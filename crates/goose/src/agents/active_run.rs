use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock, Weak},
};

use super::services::StateMachineServices;
use tokio_util::sync::CancellationToken;

struct ActiveRun {
    run_id: String,
    cancel_token: CancellationToken,
    pending_connection: Option<u64>,
    release_when_removed: bool,
}

struct SessionRunState {
    agent_run: Option<ActiveRun>,
    live_active: bool,
}

pub(crate) enum StartRunError {
    AgentRunExists { run_id: String },
    LiveVoiceInteractionExists,
    LiveVoiceInteractionMissing,
}

#[derive(Default)]
pub struct ActiveRunRegistry {
    runs_by_session: Mutex<HashMap<String, SessionRunState>>,
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
        self.insert_prompt_run(session_id, run_id, cancel_token, None)
    }

    pub(crate) fn reserve_prompt_run(
        &self,
        session_id: &str,
        connection: u64,
        run_id: String,
        cancel_token: CancellationToken,
    ) -> Result<(), StartRunError> {
        self.insert_prompt_run(session_id, run_id, cancel_token, Some(connection))
    }

    fn insert_prompt_run(
        &self,
        session_id: &str,
        run_id: String,
        cancel_token: CancellationToken,
        pending_connection: Option<u64>,
    ) -> Result<(), StartRunError> {
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
        runs.insert(
            session_id.to_string(),
            SessionRunState {
                agent_run: Some(ActiveRun {
                    run_id,
                    cancel_token,
                    pending_connection,
                    release_when_removed: false,
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
        state.agent_run = Some(ActiveRun {
            run_id,
            cancel_token,
            pending_connection: None,
            release_when_removed: false,
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

    pub(super) fn attach_prompt_run(&self, session_id: &str, connection: u64) {
        let mut runs = self
            .runs_by_session
            .lock()
            .expect("active run lock poisoned");
        if let Some(run) = runs
            .get_mut(session_id)
            .and_then(|state| state.agent_run.as_mut())
            .filter(|run| run.pending_connection == Some(connection))
        {
            run.pending_connection = None;
            run.release_when_removed = false;
        }
    }

    pub(super) fn cancel_for_release(
        &self,
        session_id: &str,
        closing_connection: Option<u64>,
    ) -> bool {
        let mut runs = self
            .runs_by_session
            .lock()
            .expect("active run lock poisoned");
        if let Some(run) = runs
            .get_mut(session_id)
            .and_then(|state| state.agent_run.as_mut())
        {
            if run
                .pending_connection
                .is_some_and(|owner| Some(owner) != closing_connection)
            {
                run.release_when_removed = true;
                return false;
            }
            run.cancel_token.cancel();
        }
        true
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
        if state
            .agent_run
            .as_ref()
            .is_none_or(|run| run.run_id != run_id)
        {
            return false;
        }
        let release_when_removed = state.agent_run.take().unwrap().release_when_removed;
        if !state.live_active {
            runs.remove(session_id);
        }
        drop(runs);
        if release_when_removed {
            if let Some(services) = self.services.get().and_then(Weak::upgrade) {
                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    let session_id = session_id.to_string();
                    handle.spawn(async move {
                        services.release_session_if_closed(&session_id).await;
                    });
                }
            }
        }
        true
    }

    pub(crate) fn start_live(&self, session_id: &str) -> bool {
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

    #[test]
    fn pending_prompt_lifetime_transfers_only_to_its_own_connection() {
        let registry = ActiveRunRegistry::default();
        let token = CancellationToken::new();
        assert!(registry
            .reserve_prompt_run("session", 2, "run".into(), token.clone())
            .is_ok());
        registry.attach_prompt_run("session", 1);
        assert!(!registry.cancel_for_release("session", Some(1)));
        assert!(!registry.cancel_for_release("session", None));
        assert!(!token.is_cancelled());
        registry.attach_prompt_run("session", 2);
        assert!(registry.cancel_for_release("session", Some(1)));
        assert!(token.is_cancelled());
    }

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
}
