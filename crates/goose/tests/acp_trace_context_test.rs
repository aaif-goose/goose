#![cfg(feature = "otel")]
#![recursion_limit = "256"]
#[allow(dead_code)]
#[path = "acp_common_tests/mod.rs"]
mod common_tests;

use agent_client_protocol::schema::v1::{
    ContentBlock, McpServer, McpServerHttp, PromptRequest, SessionUpdate, StopReason, TextContent,
    ToolCallContent,
};
use common_tests::fixtures::server::AcpServerConnection;
use common_tests::fixtures::{
    run_test, Connection, OpenAiFixture, Session, SessionData, TestConnectionConfig,
};
use common_tests::TURN_CONTEXT_OPEN;
use goose_test_support::mcp::ContextReport;
use goose_test_support::McpFixture;
use opentelemetry::trace::{SpanId, TraceId, TracerProvider as _};
use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider, SpanData};
use std::sync::OnceLock;
use std::time::Duration;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Layer as _;

const TRACESTATE: &str = "vendor=value";

/// The ACP server runs on its own runtime threads, so spans are only visible through a global subscriber.
fn exporter() -> &'static InMemorySpanExporter {
    static EXPORTER: OnceLock<InMemorySpanExporter> = OnceLock::new();
    EXPORTER.get_or_init(|| {
        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let layer = tracing_opentelemetry::layer()
            .with_tracer(provider.tracer("acp-trace-context-test"))
            .with_filter(LevelFilter::INFO);
        tracing::subscriber::set_global_default(tracing_subscriber::registry().with(layer))
            .unwrap();
        exporter
    })
}

fn exported_spans() -> Vec<SpanData> {
    exporter().get_finished_spans().unwrap()
}

fn attribute<'a>(span: &'a SpanData, key: &str) -> Option<std::borrow::Cow<'a, str>> {
    span.attributes
        .iter()
        .find(|kv| kv.key.as_str() == key)
        .map(|kv| kv.value.as_str())
}

fn is_operation(span: &SpanData, operation: &str) -> bool {
    attribute(span, "gen_ai.operation.name").as_deref() == Some(operation)
}

fn is_turn(span: &SpanData) -> bool {
    is_operation(span, "invoke_agent")
}

async fn wait_for_span(description: &str, matches: impl Fn(&SpanData) -> bool) -> SpanData {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(span) = exported_spans().into_iter().find(|span| matches(span)) {
            return span;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {description} span"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn random_remote_parent() -> (TraceId, SpanId) {
    let trace_id = TraceId::from_bytes(*uuid::Uuid::new_v4().as_bytes());
    let span_bytes = uuid::Uuid::new_v4().as_bytes()[..8].try_into().unwrap();
    (trace_id, SpanId::from_bytes(span_bytes))
}

fn prompt_meta(traceparent: &str) -> serde_json::Map<String, serde_json::Value> {
    serde_json::json!({
        "traceparent": traceparent,
        "tracestate": TRACESTATE,
    })
    .as_object()
    .unwrap()
    .clone()
}

async fn send_prompt(
    conn: &AcpServerConnection,
    session: &impl Session,
    prompt: &str,
    meta: serde_json::Map<String, serde_json::Value>,
) {
    let response = conn
        .cx()
        .send_request(
            PromptRequest::new(
                session.session_id().clone(),
                vec![ContentBlock::Text(TextContent::new(prompt))],
            )
            .meta(meta),
        )
        .block_task()
        .await
        .unwrap();
    assert_eq!(response.stop_reason, StopReason::EndTurn);
}

async fn wait_for_context_report(session: &impl Session) -> ContextReport {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let report = session.session_updates().into_iter().find_map(|update| {
            let SessionUpdate::ToolCallUpdate(update) = update else {
                return None;
            };
            update
                .fields
                .content?
                .into_iter()
                .find_map(|content| match content {
                    ToolCallContent::Content(content) => match content.content {
                        ContentBlock::Text(text) => serde_json::from_str(&text.text).ok(),
                        _ => None,
                    },
                    _ => None,
                })
        });
        if let Some(report) = report {
            return report;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for the inspect_context tool result"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[test]
fn test_prompt_trace_context_reaches_mcp_tool_call() {
    exporter();
    run_test(async move {
        let (trace_id, remote_span_id) = random_remote_parent();
        let traceparent = format!("00-{trace_id}-{remote_span_id}-01");
        let prompt = "Use the inspect_context tool.";
        let mcp = McpFixture::new().await;
        let openai = OpenAiFixture::new(
            vec![
                (
                    format!("{prompt}{TURN_CONTEXT_OPEN}"),
                    include_str!("acp_test_data/openai_inspect_context_tool_call.txt"),
                ),
                (
                    "requestTraceparent".to_string(),
                    include_str!("acp_test_data/openai_basic.txt"),
                ),
            ],
            AcpServerConnection::expected_session_id(),
        )
        .await;
        let mut conn = AcpServerConnection::new(
            TestConnectionConfig {
                mcp_servers: vec![McpServer::Http(McpServerHttp::new("mcp-fixture", &mcp.url))],
                ..Default::default()
            },
            openai,
        )
        .await;
        let SessionData { session, .. } = conn.new_session().await.unwrap();

        send_prompt(&conn, &session, prompt, prompt_meta(&traceparent)).await;
        let report = wait_for_context_report(&session).await;

        let in_trace = |span: &SpanData| span.span_context.trace_id() == trace_id;
        let turn = wait_for_span("invoke_agent", |span| in_trace(span) && is_turn(span)).await;
        let tool = wait_for_span("execute_tool", |span| {
            in_trace(span) && is_operation(span, "execute_tool")
        })
        .await;

        assert_eq!(turn.parent_span_id, remote_span_id);
        assert!(turn.parent_span_is_remote);
        assert_eq!(
            report.request_traceparent,
            Some(format!("00-{trace_id}-{}-01", tool.span_context.span_id())),
            "MCP tools/call must carry the tool span as its parent"
        );
        assert_eq!(report.request_tracestate.as_deref(), Some(TRACESTATE));
    });
}

#[test]
fn test_prompt_with_invalid_trace_context_is_ignored() {
    exporter();
    run_test(async {
        // Uppercase hex is invalid W3C, but leaves ids we can check were not adopted.
        let invalid_trace_id = "4BF92F3577B34DA6A3CE929D0E0E4736";
        let invalid_span_id = "00F067AA0BA902B7";
        let prompt = "what is 1+1";
        let openai = OpenAiFixture::new(
            vec![(
                format!("{prompt}{TURN_CONTEXT_OPEN}"),
                include_str!("acp_test_data/openai_basic.txt"),
            )],
            AcpServerConnection::expected_session_id(),
        )
        .await;
        let mut conn = AcpServerConnection::new(TestConnectionConfig::default(), openai).await;
        let SessionData { session, .. } = conn.new_session().await.unwrap();
        // Session ids repeat across tests, so only consider spans exported by this test.
        let earlier_spans = exported_spans().len();

        send_prompt(
            &conn,
            &session,
            prompt,
            prompt_meta(&format!("00-{invalid_trace_id}-{invalid_span_id}-01")),
        )
        .await;

        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let turn = loop {
            if let Some(turn) = exported_spans()
                .into_iter()
                .skip(earlier_spans)
                .find(is_turn)
            {
                break turn;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "timed out waiting for invoke_agent span"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        };

        assert!(!turn.parent_span_is_remote);
        assert_ne!(
            turn.span_context.trace_id(),
            TraceId::from_hex(invalid_trace_id).unwrap()
        );
        assert_ne!(
            turn.parent_span_id,
            SpanId::from_hex(invalid_span_id).unwrap()
        );
    });
}
