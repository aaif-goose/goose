//! Superfast Decision Gate — a small, off-by-default "System One" front-door classifier.
//!
//! Concept and reference implementation by Andrea Bruno, released under Creative
//! Commons Attribution 4.0 (CC BY 4.0). If this idea or code is adopted, please
//! keep a short credit to Andrea Bruno and a link to the harness-superfast
//! repository (https://github.com/Graphene-Lab/harness-superfast).
//!
//! The gate asks a tiny decision model (Von, OpenJev, Laya, or any Jev-compatible
//! server) three typed questions about the pending user turn in a single forward
//! pass, and derives a conservative routing recommendation. The decision models
//! themselves are third-party open models and are installed out of band; this
//! module ships only the integration architecture and the routing method.
//!
//! Design contract:
//!   - Off by default. Nothing runs unless `SUPERFAST_ENABLED` is set to a truthy
//!     value. A user who does nothing sees the exact current behaviour.
//!   - Shadow mode. This first increment only classifies and logs. It never
//!     changes routing, never skips the model call, and never alters any
//!     user-visible behaviour. Acting on the route is a later, validated step.
//!   - Fail open. Any error, timeout, non-2xx response, unreachable backend, or
//!     malformed body yields "no opinion" (`None`). The gate can only ever make
//!     the agent faster, never worse.
//!   - No heavy new dependencies. It talks to a local HTTP endpoint with the
//!     `reqwest` client this crate already uses.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{json, Value};

/// Runtime configuration for the gate, resolved from the environment.
#[derive(Debug, Clone)]
pub struct GateSettings {
    /// Master switch. When false the gate is never invoked.
    pub enabled: bool,
    /// Full URL of the decision endpoint, e.g. `http://localhost:8000/v1/systemone`.
    pub endpoint: String,
    /// Model id sent in the request body, e.g. `von-1.2.0`.
    pub model: String,
    /// Hard timeout for a single decision call.
    pub timeout: Duration,
}

impl GateSettings {
    /// Resolve settings from environment variables. Off by default.
    ///
    /// - `SUPERFAST_ENABLED`: truthy (`1`, `true`, `yes`, `on`) turns the gate on.
    /// - `SUPERFAST_ENDPOINT`: decision endpoint URL (default `http://localhost:8000/v1/systemone`).
    /// - `SUPERFAST_MODEL`: model id (default `von-1.2.0`).
    /// - `SUPERFAST_TIMEOUT_MS`: timeout in milliseconds (default `150`).
    pub fn from_env() -> Self {
        Self {
            enabled: truthy(std::env::var("SUPERFAST_ENABLED").ok().as_deref()),
            endpoint: non_empty(std::env::var("SUPERFAST_ENDPOINT").ok().as_deref())
                .unwrap_or_else(|| "http://localhost:8000/v1/systemone".to_string()),
            model: non_empty(std::env::var("SUPERFAST_MODEL").ok().as_deref())
                .unwrap_or_else(|| "von-1.2.0".to_string()),
            timeout: Duration::from_millis(
                std::env::var("SUPERFAST_TIMEOUT_MS")
                    .ok()
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .filter(|ms| *ms > 0)
                    .unwrap_or(150),
            ),
        }
    }
}

/// True only for an explicit truthy string.
fn truthy(v: Option<&str>) -> bool {
    matches!(
        v.map(|s| s.trim().to_ascii_lowercase()).as_deref(),
        Some("1") | Some("true") | Some("yes") | Some("on")
    )
}

/// Trim a string and drop it when empty.
fn non_empty(v: Option<&str>) -> Option<String> {
    v.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// A single answer returned by the decision model. Every field is optional because
/// a given question kind populates only some of them.
#[derive(Debug, Clone, Deserialize)]
struct Answer {
    /// For `choice`: the selected criterion key.
    #[serde(default)]
    choice: Option<String>,
    /// For `noul`: probability the statement is true, in `[0, 1]`.
    #[serde(default)]
    noul: Option<f64>,
    /// Calibrated confidence for choice answers.
    #[serde(default)]
    confidence: Option<f64>,
}

/// Raw response envelope from the decision endpoint.
#[derive(Debug, Deserialize)]
struct SystemOneResponse {
    #[serde(default)]
    answers: Option<HashMap<String, Answer>>,
}

/// The routing recommendation derived from a turn's decision answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnRoute {
    NeedsTool,
    AnswerFromContext,
    PlainChat,
    Unknown,
}

impl TurnRoute {
    fn as_str(self) -> &'static str {
        match self {
            TurnRoute::NeedsTool => "needs_tool",
            TurnRoute::AnswerFromContext => "answer_from_context",
            TurnRoute::PlainChat => "plain_chat",
            TurnRoute::Unknown => "unknown",
        }
    }
}

/// A turn-level decision: the derived route plus measured latency.
#[derive(Debug, Clone, Copy)]
pub struct TurnDecision {
    pub route: TurnRoute,
    pub latency_ms: u64,
}

/// Reuse a single HTTP client (and its connection pool) across decision calls.
fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

/// Standard question set for classifying an incoming user turn. Kept small so the
/// single forward pass stays well under the timeout budget.
fn turn_questions() -> Value {
    json!({
        "needs_tool": {
            "type": "noul",
            "instructions": "Does answering this request require taking an action with a tool (reading, writing, running, searching), rather than replying from what is already known?"
        },
        "answerable_from_context": {
            "type": "noul",
            "instructions": "Can this request be answered from information already present in the conversation, without any new investigation?"
        },
        "intent": {
            "type": "choice",
            "instructions": "Classify the primary intent of the user request.",
            "criteria": {
                "code_change": "Create, edit, or delete code or files.",
                "code_question": "Explain or reason about code without changing it.",
                "command": "Run a command or operation.",
                "chat": "Casual conversation or a question needing no tools.",
                "other": "None of the above."
            }
        }
    })
}

/// Issue one System One request. Returns the parsed answers on success, or `None`
/// on any failure (fail-open). Never panics.
async fn query_system_one(state: &str, settings: &GateSettings) -> Option<HashMap<String, Answer>> {
    let body = json!({
        "model": settings.model,
        "state": state,
        "questions": turn_questions(),
    });

    let res = http_client()
        .post(&settings.endpoint)
        .timeout(settings.timeout)
        .json(&body)
        .send()
        .await
        .ok()?;

    if !res.status().is_success() {
        return None;
    }

    let parsed: SystemOneResponse = res.json().await.ok()?;
    parsed.answers
}

/// Read a noul probability, returning it only when it is a real, finite value in
/// the closed `[0, 1]` interval. Anything else (absent, NaN, Infinity, out of
/// range) is treated as "no evidence", so a mis-scaled or missing answer can
/// never produce a decisive fast route.
fn read_noul(answer: Option<&Answer>) -> Option<f64> {
    let v = answer?.noul?;
    if v.is_finite() && (0.0..=1.0).contains(&v) {
        Some(v)
    } else {
        None
    }
}

/// True only for a real, finite confidence in `[0, 1]` at or above the floor.
fn confident(answer: Option<&Answer>, floor: f64) -> bool {
    matches!(answer.and_then(|a| a.confidence), Some(c) if c.is_finite() && (0.0..=1.0).contains(&c) && c >= floor)
}

/// Derive a conservative route from the answers. The gate only recommends a fast
/// route when the relevant probabilities are decisive; otherwise it says
/// `unknown` so the caller falls back to the normal path.
fn derive_route(answers: &HashMap<String, Answer>) -> TurnRoute {
    let needs_tool = read_noul(answers.get("needs_tool"));
    let from_context = read_noul(answers.get("answerable_from_context"));
    let intent = answers.get("intent");

    // Decisive "needs a tool" wins first — the harness must not skip work.
    if needs_tool.map_or(false, |v| v >= 0.85) {
        return TurnRoute::NeedsTool;
    }

    // Strongly answerable from context, with a present and low tool-need signal.
    if from_context.map_or(false, |v| v >= 0.85) && needs_tool.map_or(false, |v| v <= 0.3) {
        return TurnRoute::AnswerFromContext;
    }

    // Clearly chat, with a calibrated intent and a present, low tool-need signal.
    if intent.and_then(|a| a.choice.as_deref()) == Some("chat")
        && confident(intent, 0.5)
        && needs_tool.map_or(false, |v| v <= 0.2)
    {
        return TurnRoute::PlainChat;
    }

    TurnRoute::Unknown
}

/// Classify a user turn through the gate. Returns `None` when the gate is disabled
/// or unavailable (fail-open). Otherwise returns a [`TurnDecision`] whose `route`
/// is a conservative recommendation the caller may act on later.
pub async fn classify_turn(state: &str, settings: &GateSettings) -> Option<TurnDecision> {
    if !settings.enabled {
        return None;
    }
    let started = Instant::now();
    let answers = query_system_one(state, settings).await?;
    Some(TurnDecision {
        route: derive_route(&answers),
        latency_ms: started.elapsed().as_millis() as u64,
    })
}

/// Shadow-mode entry point. When the gate is enabled, classify the user turn on a
/// detached task and log the recommendation and latency. This never blocks the
/// agent turn and never changes behaviour; it only measures. When disabled it
/// returns immediately with no cost.
pub fn shadow_classify(user_message: String) {
    let settings = GateSettings::from_env();
    if !settings.enabled {
        return;
    }
    tokio::spawn(async move {
        match classify_turn(&user_message, &settings).await {
            Some(decision) => tracing::info!(
                route = decision.route.as_str(),
                latency_ms = decision.latency_ms,
                "superfast decision gate (shadow): recommendation only, no routing change"
            ),
            None => {
                tracing::debug!("superfast decision gate (shadow): no opinion (fail-open)")
            }
        }
    });
}
