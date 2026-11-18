//! W3C trace context carried in ACP and MCP `_meta` (MCP SEP-414 keys).

use opentelemetry::propagation::TextMapPropagator;
use opentelemetry::trace::TraceContextExt;
use opentelemetry::Context;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use serde_json::{Map, Value};
use std::collections::HashMap;

const TRACEPARENT: &str = "traceparent";
const TRACESTATE: &str = "tracestate";

/// Returns the remote context from `_meta.traceparent`/`tracestate`, or `None` if absent or invalid.
/// Only the lowercase SEP-414 keys are read, so mixed-case duplicates cannot race.
pub fn extract_from_meta(meta: Option<&Map<String, Value>>) -> Option<Context> {
    let meta = meta?;
    let carrier: HashMap<String, String> = [TRACEPARENT, TRACESTATE]
        .into_iter()
        .filter_map(|key| Some((key.to_string(), meta.get(key)?.as_str()?.to_string())))
        .collect();
    let cx = TraceContextPropagator::new().extract_with_context(&Context::new(), &carrier);
    cx.span().span_context().is_valid().then_some(cx)
}

/// Writes the current OpenTelemetry context into `meta`; no-op when there is no valid span context.
pub fn inject_current(meta: &mut Map<String, Value>) {
    let mut carrier = HashMap::new();
    TraceContextPropagator::new().inject_context(&Context::current(), &mut carrier);
    meta.extend(
        carrier
            .into_iter()
            .filter(|(_, value)| !value.is_empty())
            .map(|(key, value)| (key, Value::String(value))),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry::trace::{
        SpanContext, SpanId, TraceFlags, TraceId, TraceState, TracerProvider,
    };
    use opentelemetry_sdk::trace::SdkTracerProvider;
    use serde_json::json;
    use test_case::test_case;
    use tracing_opentelemetry::OpenTelemetrySpanExt;
    use tracing_subscriber::layer::SubscriberExt;

    const TRACE_ID: &str = "4bf92f3577b34da6a3ce929d0e0e4736";
    const SPAN_ID: &str = "00f067aa0ba902b7";
    const PARENT: &str = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";

    fn meta(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    fn remote_context() -> Context {
        Context::new().with_remote_span_context(SpanContext::new(
            TraceId::from_hex(TRACE_ID).unwrap(),
            SpanId::from_hex(SPAN_ID).unwrap(),
            TraceFlags::SAMPLED,
            true,
            "vendor=value".parse::<TraceState>().unwrap(),
        ))
    }

    #[test]
    fn extracts_remote_context() {
        let value = json!({ "traceparent": PARENT, "tracestate": "vendor=value" });
        let cx = extract_from_meta(Some(&meta(value))).unwrap();
        let span_context = cx.span().span_context().clone();

        assert_eq!(span_context.trace_id().to_string(), TRACE_ID);
        assert_eq!(span_context.span_id().to_string(), SPAN_ID);
        assert!(span_context.is_remote());
        assert!(span_context.is_sampled());
        assert_eq!(span_context.trace_state().header(), "vendor=value");
    }

    #[test_case(None; "no meta")]
    #[test_case(Some(json!({ "tracestate": "vendor=value" })); "no traceparent")]
    #[test_case(Some(json!({ "traceparent": "not-a-traceparent" })); "malformed")]
    #[test_case(Some(json!({ "traceparent": "00-00000000000000000000000000000000-00f067aa0ba902b7-01" })); "zero trace id")]
    #[test_case(Some(json!({ "traceparent": 42 })); "not a string")]
    #[test_case(Some(json!({ "TraceParent": PARENT })); "non lowercase key")]
    fn ignores_missing_or_invalid_context(value: Option<Value>) {
        let meta = value.map(meta);
        assert!(extract_from_meta(meta.as_ref()).is_none());
    }

    #[test]
    fn injects_nothing_without_current_context() {
        let mut meta = Map::new();
        inject_current(&mut meta);
        assert!(meta.is_empty());
    }

    #[test]
    fn relays_attached_remote_context_unchanged() {
        let _guard = remote_context().attach();
        let mut meta = Map::new();
        inject_current(&mut meta);

        assert_eq!(meta[TRACEPARENT], PARENT);
        assert_eq!(meta[TRACESTATE], "vendor=value");
    }

    #[test]
    fn injects_entered_tracing_span_as_parent() {
        let provider = SdkTracerProvider::builder().build();
        let subscriber = tracing_subscriber::registry()
            .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));

        tracing::subscriber::with_default(subscriber, || {
            let _remote = remote_context().attach();
            let span = tracing::info_span!("tool");
            let _entered = span.enter();
            let span_id = span.context().span().span_context().span_id();
            let mut meta = Map::new();
            inject_current(&mut meta);

            assert_eq!(meta[TRACEPARENT], format!("00-{TRACE_ID}-{span_id}-01"),);
            assert_eq!(meta[TRACESTATE], "vendor=value");
        });
    }
}
