use std::sync::{Arc, Mutex};

pub const TEST_SESSION_ID: &str = "test-session-id";
// Use a Chat Completions model so the canned SSE fixtures (which return
// Chat Completions format) are parsed correctly. gpt-5-nano now routes to
// the Responses API which needs a different mock format.
// TODO: add a Responses API mock to OpenAiFixture so tests can cover
// responses-routed models like gpt-5-nano end-to-end.
pub const TEST_MODEL: &str = "gpt-4.1";

const NOT_YET_SET: &str = "session-id-not-yet-set";
pub(crate) const SESSION_ID_HEADER: &str = "agent-session-id";

pub trait ExpectedSessionId: Send + Sync {
    fn set(&self, id: &str);
    fn validate(&self, actual: Option<&str>) -> Result<(), String>;
    fn assert_matches(&self, actual: &str);
}

#[derive(Clone)]
pub struct EnforceSessionId {
    value: Arc<Mutex<String>>,
    errors: Arc<Mutex<Vec<String>>>,
}

impl Default for EnforceSessionId {
    fn default() -> Self {
        Self {
            value: Arc::new(Mutex::new(NOT_YET_SET.to_string())),
            errors: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl ExpectedSessionId for EnforceSessionId {
    fn set(&self, id: &str) {
        *self.value.lock().unwrap() = id.into();
    }

    fn validate(&self, actual: Option<&str>) -> Result<(), String> {
        let expected = self.value.lock().unwrap();
        let err = match actual {
            Some(act) if act == *expected => None,
            _ => Some(format!(
                "{} mismatch: expected '{}', got {:?}",
                SESSION_ID_HEADER, expected, actual
            )),
        };
        match err {
            Some(e) => {
                self.errors.lock().unwrap().push(e.clone());
                Err(e)
            }
            None => Ok(()),
        }
    }

    fn assert_matches(&self, actual: &str) {
        let result = self.validate(Some(actual));
        assert!(result.is_ok(), "{}", result.unwrap_err());
        let errors = self.errors.lock().unwrap();
        assert!(
            errors.is_empty(),
            "Session ID validation errors: {:?}",
            *errors
        );
    }
}

#[derive(Clone)]
pub struct IgnoreSessionId;

impl ExpectedSessionId for IgnoreSessionId {
    fn set(&self, _id: &str) {}
    fn validate(&self, _actual: Option<&str>) -> Result<(), String> {
        Ok(())
    }
    fn assert_matches(&self, _actual: &str) {}
}
