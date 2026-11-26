use std::{
    collections::{HashMap, HashSet},
    sync::{Mutex, OnceLock, Weak},
};

use super::services::StateMachineServices;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

struct ActiveRun {
    run_id: String,
    cancel_token: CancellationToken,
    completion: watch::Sender<bool>,
}

struct SessionRunState {
    agent_run: Option<ActiveRun>,
    live_active: bool,
}

pub(crate) enum StartRunError {
    AgentRunExists { run_id: String },
    LiveVoiceInteractionExists,
    LiveVoiceInteractionMissing,
    SessionDeleting,
}

#[derive(Default)]
pub struct ActiveRunRegistry {
    runs_by_session: Mutex<HashMap<String, SessionRunState>>,
    deleting: Mutex<HashSet<String>>,
    services: OnceLock<Weak<StateMachineServices>>,
}

impl ActiveRunRegistry {
    pub(super) fn set_services(&self, services: Weak<StateMachineServices>) {
        self.services
            .set(services)
            .unwrap_or_else(|_| panic!("active run services already set"));
    }

    pub(crate) fn start_prompt_run(
        &self,
        session_id: &str,
        run_id: String,
        cancel_token: CancellationToken,
    ) -> Result<(), StartRunError> {
        let deleting = self.deleting.lock().expect("deleting lock poisoned");
        if deleting.contains(session_id) {
            return Err(StartRunError::SessionDeleting);
        }
        // The gate stays held while the run registers (lock order: deleting,
        // then runs), so a concurrent `begin_session_delete` either tracks
        // this run in its cancellation pass or this prompt is rejected.
        let mut runs = self
            .runs_by_session
            .lock()
            .expect("active run lock poisoned");
        if let Some(state) = runs.get(session_id) {
            if let Some(agent_run) = &state.agent_run {
                return Err(StartRunError::AgentRunExists {
                    run_id: agent_run.run_id.clone(),
                });
            }
            if state.live_active {
                return Err(StartRunError::LiveVoiceInteractionExists);
            }
        }
        let (completion, _) = watch::channel(false);
        runs.insert(
            session_id.to_string(),
            SessionRunState {
                agent_run: Some(ActiveRun {
                    run_id,
                    cancel_token,
                    completion,
                }),
                live_active: false,
            },
        );
        Ok(())
    }

    pub(crate) fn start_live_delegation(
        &self,
        session_id: &str,
        run_id: String,
        cancel_token: CancellationToken,
    ) -> Result<(), StartRunError> {
        let deleting = self.deleting.lock().expect("deleting lock poisoned");
        if deleting.contains(session_id) {
            return Err(StartRunError::SessionDeleting);
        }
        // Same lock order as `start_prompt_run`: the gate stays held while
        // the delegation registers.
        let mut runs = self
            .runs_by_session
            .lock()
            .expect("active run lock poisoned");
        let Some(state) = runs.get_mut(session_id) else {
            return Err(StartRunError::LiveVoiceInteractionMissing);
        };
        if let Some(agent_run) = &state.agent_run {
            return Err(StartRunError::AgentRunExists {
                run_id: agent_run.run_id.clone(),
            });
        }
        let (completion, _) = watch::channel(false);
        state.agent_run = Some(ActiveRun {
            run_id,
            cancel_token,
            completion,
        });
        Ok(())
    }

    pub(crate) fn agent_run_id(&self, session_id: &str) -> Option<String> {
        self.runs_by_session
            .lock()
            .expect("active run lock poisoned")
            .get(session_id)
            .and_then(|state| state.agent_run.as_ref())
            .map(|run| run.run_id.clone())
    }

    pub(crate) fn agent_cancel_token(&self, session_id: &str) -> Option<CancellationToken> {
        self.runs_by_session
            .lock()
            .expect("active run lock poisoned")
            .get(session_id)
            .and_then(|state| state.agent_run.as_ref())
            .map(|run| run.cancel_token.clone())
    }

    pub(crate) fn cancel_agent_run(&self, session_id: &str) {
        if let Some(cancel_token) = self.agent_cancel_token(session_id) {
            cancel_token.cancel();
        }
    }

    /// Cancels the session's active run and returns a watcher that resolves
    /// once the run signals completion. Unlike `cancel_agent_run`, this
    /// distinguishes signalling cancellation from the prompt's final session
    /// work having finished.
    pub(crate) fn cancel_agent_run_and_track(
        &self,
        session_id: &str,
    ) -> Option<watch::Receiver<bool>> {
        let runs = self
            .runs_by_session
            .lock()
            .expect("active run lock poisoned");
        let run = runs.get(session_id)?.agent_run.as_ref()?;
        run.cancel_token.cancel();
        Some(run.completion.subscribe())
    }

    /// Closes prompt admission for these sessions and cancels the runs that
    /// are already registered as one step: a prompt that passed the gate has
    /// registered before this returns, so it is tracked here, and anything
    /// that tries afterwards is rejected until `clear_session_deleting`.
    pub(crate) fn begin_session_delete(
        &self,
        session_ids: &[String],
    ) -> Vec<(String, watch::Receiver<bool>)> {
        let mut deleting = self.deleting.lock().expect("deleting lock poisoned");
        deleting.extend(session_ids.iter().cloned());
        // The gate stays held over the cancellation pass (lock order:
        // deleting, then runs), so a prompt that passed the gate is already
        // registered and gets tracked here.
        session_ids
            .iter()
            .filter_map(|session_id| {
                self.cancel_agent_run_and_track(session_id)
                    .map(|completion| (session_id.clone(), completion))
            })
            .collect()
    }

    pub(crate) fn clear_session_deleting(&self, session_id: &str) {
        self.deleting
            .lock()
            .expect("deleting lock poisoned")
            .remove(session_id);
    }

    /// Signals the run's completion watcher while the run is still
    /// registered. Prompt paths call this after their final session work;
    /// registration removal alone happens earlier and is not a completion
    /// signal.
    pub(crate) fn complete_agent_run(&self, session_id: &str, run_id: &str) {
        let runs = self
            .runs_by_session
            .lock()
            .expect("active run lock poisoned");
        if let Some(state) = runs.get(session_id) {
            if let Some(run) = state.agent_run.as_ref() {
                if run.run_id == run_id {
                    let _ = run.completion.send_replace(true);
                }
            }
        }
    }

    /// Whether the session's run was `run_id` and is now removed.
    pub(crate) fn remove_agent_run(&self, session_id: &str, run_id: &str) -> bool {
        let mut runs = self
            .runs_by_session
            .lock()
            .expect("active run lock poisoned");
        let Some(state) = runs.get_mut(session_id) else {
            return false;
        };
        let Some(run) = state.agent_run.take() else {
            return false;
        };
        if run.run_id != run_id {
            state.agent_run = Some(run);
            return false;
        }
        let _ = run.completion.send_replace(true);
        if !state.live_active {
            runs.remove(session_id);
        }
        true
    }

    /// Reserves the session for a Live interaction. Returns false while the
    /// session is being deleted or already has a run.
    pub(crate) fn start_live(&self, session_id: &str) -> bool {
        let deleting = self.deleting.lock().expect("deleting lock poisoned");
        if deleting.contains(session_id) {
            return false;
        }
        // Same lock order as `start_prompt_run`: the gate stays held while
        // the reservation registers.
        let mut runs = self
            .runs_by_session
            .lock()
            .expect("active run lock poisoned");
        if runs.contains_key(session_id) {
            return false;
        }
        runs.insert(
            session_id.to_string(),
            SessionRunState {
                agent_run: None,
                live_active: true,
            },
        );
        true
    }

    pub(crate) fn finish_live(&self, session_id: &str) {
        let mut runs = self
            .runs_by_session
            .lock()
            .expect("active run lock poisoned");
        if let Some(state) = runs.get_mut(session_id) {
            state.live_active = false;
            if state.agent_run.is_none() {
                runs.remove(session_id);
            }
        }
        drop(runs);
        if let Some(services) = self.services.get().and_then(Weak::upgrade) {
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move { services.trim_sessions().await });
            }
        }
    }

    pub(crate) fn is_active(&self, session_id: &str) -> bool {
        self.runs_by_session
            .lock()
            .expect("active run lock poisoned")
            .contains_key(session_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn live_runs_are_scoped_by_session() {
        let registry = ActiveRunRegistry::default();

        assert!(registry.start_live("one"));
        assert!(!registry.start_live("one"));
        assert!(registry.start_live("two"));

        registry.finish_live("one");
        assert!(registry.start_live("one"));
    }

    #[tokio::test]
    async fn prompt_and_live_runs_conflict() {
        let registry = ActiveRunRegistry::default();

        assert!(registry
            .start_prompt_run("session", "run".into(), CancellationToken::new(),)
            .is_ok());
        assert!(!registry.start_live("session"));

        registry.remove_agent_run("session", "run");
        assert!(registry.start_live("session"));
        assert!(matches!(
            registry.start_prompt_run("session", "run".into(), CancellationToken::new(),),
            Err(StartRunError::LiveVoiceInteractionExists)
        ));
    }

    #[tokio::test]
    async fn live_delegation_shares_the_live_session_without_admitting_another_prompt() {
        let registry = ActiveRunRegistry::default();
        assert!(registry.start_live("session"));
        assert!(registry
            .start_live_delegation("session", "delegated".into(), CancellationToken::new(),)
            .is_ok());
        assert!(matches!(
            registry.start_prompt_run("session", "prompt".into(), CancellationToken::new(),),
            Err(StartRunError::AgentRunExists { .. })
        ));

        registry.finish_live("session");
        assert_eq!(registry.agent_run_id("session"), Some("delegated".into()));
    }

    #[tokio::test]
    async fn cancel_and_track_resolves_when_the_run_leaves_the_registry() {
        let registry = ActiveRunRegistry::default();
        assert!(registry
            .start_prompt_run("session", "run".into(), CancellationToken::new(),)
            .is_ok());

        let mut completion = registry.cancel_agent_run_and_track("session").unwrap();
        assert!(!*completion.borrow_and_update());

        assert!(registry.remove_agent_run("session", "run"));
        completion.changed().await.unwrap();
        assert!(*completion.borrow_and_update());
    }

    #[tokio::test]
    async fn complete_agent_run_signals_the_watcher_while_registered() {
        let registry = ActiveRunRegistry::default();
        assert!(registry
            .start_prompt_run("session", "run".into(), CancellationToken::new(),)
            .is_ok());
        let mut completion = registry.cancel_agent_run_and_track("session").unwrap();

        registry.complete_agent_run("session", "run");
        assert!(*completion.borrow_and_update());
        assert_eq!(registry.agent_run_id("session"), Some("run".into()));
    }

    #[tokio::test]
    async fn deleting_gate_blocks_new_prompts_until_cleared() {
        let registry = ActiveRunRegistry::default();
        assert!(registry
            .start_prompt_run("session", "run".into(), CancellationToken::new())
            .is_ok());

        let mut tracked = registry.begin_session_delete(&["session".to_string()]);
        // The run that was already registered is cancelled and tracked.
        assert_eq!(tracked.len(), 1);
        assert!(!*tracked[0].1.borrow_and_update());
        assert!(matches!(
            registry.start_prompt_run("session", "second".into(), CancellationToken::new()),
            Err(StartRunError::SessionDeleting)
        ));

        registry.clear_session_deleting("session");
        registry.remove_agent_run("session", "run");
        assert!(*tracked[0].1.borrow_and_update());
        assert!(registry
            .start_prompt_run("session", "second".into(), CancellationToken::new())
            .is_ok());
    }

    #[test]
    fn begin_session_delete_leaves_no_untracked_run() {
        let registry = Arc::new(ActiveRunRegistry::default());
        let writers: Vec<_> = (0..8)
            .map(|writer| {
                let registry = Arc::clone(&registry);
                std::thread::spawn(move || {
                    for round in 0..200 {
                        let run_id = format!("run-{writer}-{round}");
                        if registry
                            .start_prompt_run("session", run_id.clone(), CancellationToken::new())
                            .is_ok()
                        {
                            assert!(registry.remove_agent_run("session", &run_id));
                        }
                    }
                })
            })
            .collect();

        let tracked = registry.begin_session_delete(&["session".to_string()]);
        // A prompt that passed the gate registered before it closed, so it is
        // the only run that can be tracked; nothing registers afterwards.
        assert!(tracked.len() <= 1);
        assert!(matches!(
            registry.start_prompt_run("session", "late".into(), CancellationToken::new()),
            Err(StartRunError::SessionDeleting)
        ));
        for writer in writers {
            writer.join().unwrap();
        }
    }

    #[tokio::test]
    async fn deleting_gate_blocks_live_reservations_and_delegations() {
        let registry = ActiveRunRegistry::default();
        registry.begin_session_delete(&["closed".to_string()]);
        assert!(!registry.start_live("closed"));

        assert!(registry.start_live("open"));
        registry.begin_session_delete(&["open".to_string()]);
        assert!(matches!(
            registry.start_live_delegation("open", "delegated".into(), CancellationToken::new()),
            Err(StartRunError::SessionDeleting)
        ));
    }
}
