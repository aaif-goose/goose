use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use indexmap::IndexMap;
use tokio::sync::mpsc;

use super::active_run::ActiveRunRegistry;
use super::extension_manager::ClientContext;
use super::state_machine::SteerQueue;
use super::tool_confirmation_coordinator::ToolConfirmationCoordinator;
use crate::conversation::message::Message;
use crate::session::SessionNameUpdate;

/// A client connection that has a session open.
#[derive(Clone)]
pub struct Attachment {
    pub client: ClientContext,
    pub session_names: Option<mpsc::UnboundedSender<SessionNameUpdate>>,
}

/// Everything goose keeps for a live session besides its record: which connections have it
/// open, whether it is resident, its run, confirmations and pending steers.
pub struct SessionRuntime {
    max_sessions: usize,
    resident: Mutex<lru::LruCache<String, ()>>,
    /// Per session, in attach order; the last one is who the session currently talks to.
    attachments: Mutex<HashMap<String, IndexMap<u64, Attachment>>>,
    steer_queues: Mutex<HashMap<String, SteerQueue>>,
    lifetimes: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    pub(super) confirmations: ToolConfirmationCoordinator,
    pub runs: Arc<ActiveRunRegistry>,
}

impl SessionRuntime {
    pub(super) fn new(max_sessions: usize) -> Self {
        Self {
            max_sessions: max_sessions.max(1),
            resident: Mutex::new(lru::LruCache::unbounded()),
            attachments: Mutex::new(HashMap::new()),
            steer_queues: Mutex::new(HashMap::new()),
            lifetimes: Mutex::new(HashMap::new()),
            confirmations: ToolConfirmationCoordinator::new(),
            runs: Arc::new(ActiveRunRegistry::default()),
        }
    }

    /// Marks the session as in use and returns the sessions to release to stay within
    /// `max_sessions`: least recently used first, never `session_id` itself and never a
    /// busy one. When every other session is busy the limit is exceeded until a turn ends
    /// or another session is touched.
    pub(super) fn touch(&self, session_id: &str) -> Vec<String> {
        let mut resident = self.resident.lock().expect("session runtime poisoned");
        resident.put(session_id.to_string(), ());
        self.over_limit(&mut resident, Some(session_id))
    }

    pub(super) fn trim(&self) -> Vec<String> {
        let mut resident = self.resident.lock().expect("session runtime poisoned");
        self.over_limit(&mut resident, None)
    }

    fn over_limit(
        &self,
        resident: &mut lru::LruCache<String, ()>,
        keep: Option<&str>,
    ) -> Vec<String> {
        let excess = resident.len().saturating_sub(self.max_sessions);
        let evicted: Vec<String> = resident
            .iter()
            .rev()
            .map(|(id, ())| id.clone())
            .filter(|id| Some(id.as_str()) != keep && !self.is_busy(id))
            .take(excess)
            .collect();
        for id in &evicted {
            resident.pop(id);
        }
        evicted
    }

    /// Held while a session is attached or released, so a release never clears a session
    /// that was attached while it ran.
    pub(super) async fn lock_lifetime(&self, session_id: &str) -> LifetimeGuard<'_> {
        let lock = self
            .lifetimes
            .lock()
            .expect("session runtime poisoned")
            .entry(session_id.to_string())
            .or_default()
            .clone();
        LifetimeGuard {
            runtime: self,
            session_id: session_id.to_string(),
            _held: lock.lock_owned().await,
        }
    }

    pub(super) fn attach(&self, session_id: &str, connection: u64, attachment: Attachment) {
        let mut attachments = self.attachments.lock().expect("session runtime poisoned");
        let connections = attachments.entry(session_id.to_string()).or_default();
        connections.shift_remove(&connection);
        connections.insert(connection, attachment);
    }

    pub fn is_attached(&self, session_id: &str, connection: u64) -> bool {
        self.attachments
            .lock()
            .expect("session runtime poisoned")
            .get(session_id)
            .is_some_and(|connections| connections.contains_key(&connection))
    }

    pub fn is_open(&self, session_id: &str) -> bool {
        self.attachments
            .lock()
            .expect("session runtime poisoned")
            .contains_key(session_id)
    }

    /// Removes the connection from the session and returns who the session talks to now,
    /// `None` when no connection has it open any more.
    pub(super) fn detach(&self, session_id: &str, connection: u64) -> Option<Attachment> {
        let mut attachments = self.attachments.lock().expect("session runtime poisoned");
        let connections = attachments.get_mut(session_id)?;
        connections.shift_remove(&connection);
        let latest = connections.last().map(|(_, attachment)| attachment.clone());
        if latest.is_none() {
            attachments.remove(session_id);
        }
        latest
    }

    /// Removes the connection from every session it had open; returns, per session that
    /// still has a connection, the one it talks to now.
    pub(super) fn detach_connection(&self, connection: u64) -> Vec<(String, Attachment)> {
        let mut attachments = self.attachments.lock().expect("session runtime poisoned");
        let mut repointed = Vec::new();
        attachments.retain(|session_id, connections| {
            let was_latest = connections.last().is_some_and(|(id, _)| *id == connection);
            connections.shift_remove(&connection);
            if was_latest {
                if let Some((_, attachment)) = connections.last() {
                    repointed.push((session_id.clone(), attachment.clone()));
                }
            }
            !connections.is_empty()
        });
        repointed
    }

    pub(super) fn session_names(
        &self,
        session_id: &str,
    ) -> Option<mpsc::UnboundedSender<SessionNameUpdate>> {
        self.attachments
            .lock()
            .expect("session runtime poisoned")
            .get(session_id)
            .and_then(|connections| connections.last())
            .and_then(|(_, attachment)| attachment.session_names.clone())
    }

    pub(super) fn steer_queue(&self, session_id: &str) -> SteerQueue {
        self.steer_queues
            .lock()
            .expect("session runtime poisoned")
            .entry(session_id.to_string())
            .or_default()
            .clone()
    }

    pub(super) async fn steer(&self, session_id: &str, message: Message) {
        self.steer_queue(session_id).lock().await.push_back(message);
    }

    pub(super) fn discard_pending_steers(&self, session_id: &str) {
        self.steer_queues
            .lock()
            .expect("session runtime poisoned")
            .remove(session_id);
    }

    /// Whether an eviction candidate must stay: touched again since it was chosen, or busy
    /// (then it goes back to the resident set).
    pub(super) fn keep_resident(&self, session_id: &str) -> bool {
        let mut resident = self.resident.lock().expect("session runtime poisoned");
        if resident.contains(session_id) {
            return true;
        }
        if self.is_busy(session_id) {
            resident.put(session_id.to_string(), ());
            resident.demote(session_id);
            return true;
        }
        false
    }

    /// A Live conversation runs without a turn, and a prompt registers its run before
    /// its turn starts.
    fn is_busy(&self, session_id: &str) -> bool {
        self.confirmations.has_active_turn(session_id) || self.runs.is_active(session_id)
    }

    /// Drops everything kept for the session and cancels its run.
    pub(super) fn forget(&self, session_id: &str) {
        self.resident
            .lock()
            .expect("session runtime poisoned")
            .pop(session_id);
        self.attachments
            .lock()
            .expect("session runtime poisoned")
            .remove(session_id);
        self.discard_pending_steers(session_id);
        self.confirmations.release(session_id);
        self.runs.cancel_agent_run(session_id);
    }
}

pub(super) struct LifetimeGuard<'a> {
    runtime: &'a SessionRuntime,
    session_id: String,
    _held: tokio::sync::OwnedMutexGuard<()>,
}

impl Drop for LifetimeGuard<'_> {
    fn drop(&mut self) {
        let mut lifetimes = self
            .runtime
            .lifetimes
            .lock()
            .expect("session runtime poisoned");
        // The map's reference and ours: nobody else is waiting.
        if lifetimes
            .get(&self.session_id)
            .is_some_and(|lock| Arc::strong_count(lock) == 2)
        {
            lifetimes.remove(&self.session_id);
        }
    }
}
