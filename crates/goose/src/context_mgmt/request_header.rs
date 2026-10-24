//! Last routed request header per session, recorded so the compaction call
//! can replay the same cacheable prefix.

use crate::providers::base::Provider;
use rmcp::model::Tool;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, OnceLock, Weak};

/// Evicted (or never-recorded) sessions summarize without a header.
const MAX_SESSIONS: usize = 64;

#[derive(Clone, Default)]
pub struct RequestHeader {
    pub system_prompt: String,
    pub tools: Vec<Tool>,
    /// Real tool definitions when toolshim is active (`tools` is empty then):
    /// the summarizer's response must be interpreted the same way as the
    /// session's own responses.
    pub toolshim_tools: Vec<Tool>,
}

/// Each header is bound to the provider instance it was routed through.
/// Switching providers mid-session (`/model --provider ...`) leaves a header
/// describing the old provider's wire shape, which is worse than no header at
/// all, and session ids are only unique per session store, so agents sharing a
/// process must not see each other's headers. The `Weak` keeps the provider's
/// allocation reserved, so a later provider can never reuse its address.
#[derive(Default)]
struct Registry {
    headers: HashMap<String, (Weak<dyn Provider>, RequestHeader)>,
    insertion_order: VecDeque<String>,
}

fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Registry::default()))
}

pub fn record(session_id: &str, provider: &Arc<dyn Provider>, header: RequestHeader) {
    let mut registry = registry().lock().unwrap();
    if !registry.headers.contains_key(session_id) {
        while registry.insertion_order.len() >= MAX_SESSIONS {
            if let Some(oldest) = registry.insertion_order.pop_front() {
                registry.headers.remove(&oldest);
            }
        }
        registry.insertion_order.push_back(session_id.to_string());
    }
    registry
        .headers
        .insert(session_id.to_string(), (Arc::downgrade(provider), header));
}

/// `None` when the session last routed through a different provider instance.
pub fn last_for_session(session_id: &str, provider: &dyn Provider) -> Option<RequestHeader> {
    registry()
        .lock()
        .unwrap()
        .headers
        .get(session_id)
        .filter(|(recorded, _)| std::ptr::addr_eq(recorded.as_ptr(), provider))
        .map(|(_, header)| header.clone())
}
