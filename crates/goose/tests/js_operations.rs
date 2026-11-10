#![cfg(feature = "js-operations")]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use goose::agents::js_operations::{
    load_operations, place_operations, ConfiguredOperation, JavaScriptOperation, OperationPlacement,
};
use goose::agents::state_machine::{
    ConversationEffect, Emitter, GooseEffect, Inference, InferenceInput, Operation,
    OperationResult, RunStatus, SlashCommand, StateMachine, Step,
};
use goose::agents::AgentEvent;
use goose::config::{Config, GooseMode};
use goose::conversation::message::Message;
use goose::session::session_manager::SessionType;
use goose::session::{Session, SessionManager};
use rmcp::model::Tool;
use serde_json::json;
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

async fn session() -> Result<(TempDir, SessionManager, Session)> {
    let directory = TempDir::new()?;
    let manager = SessionManager::new(directory.path().join("sessions"));
    let session = manager
        .create_session(
            directory.path().to_path_buf(),
            "JavaScript operation".into(),
            SessionType::Hidden,
            GooseMode::default(),
        )
        .await?;
    manager
        .add_message(&session.id, &Message::user().with_text("hello"))
        .await?;
    let session = manager.get_session(&session.id, true).await?;
    Ok((directory, manager, session))
}

async fn operation(source: &str) -> Result<(TempDir, JavaScriptOperation)> {
    let directory = TempDir::new()?;
    let path = directory.path().join("operation.js");
    tokio::fs::write(&path, source).await?;
    let operation = JavaScriptOperation::from_file(path).await?;
    Ok((directory, operation))
}

#[tokio::test]
async fn configuration_preserves_order_and_rejects_duplicates_and_invalid_values() -> Result<()> {
    let directory = TempDir::new()?;
    let config = Config::new_with_file_secrets(
        directory.path().join("config.yaml"),
        directory.path().join("secrets.yaml"),
    )?;
    assert!(load_operations(&config).await?.is_empty());
    let first = directory.path().join("first.js");
    let second = directory.path().join("second.js");
    tokio::fs::write(&first, "const operation = { name: 'first' };").await?;
    tokio::fs::write(&second, "const operation = { name: 'second' };").await?;
    config.set_param("GOOSE_JS_OPERATIONS", [&second, &first])?;
    let loaded = load_operations(&config).await?;
    assert_eq!(
        loaded
            .iter()
            .map(|op| op.operation.name())
            .collect::<Vec<_>>(),
        ["js:second", "js:first"]
    );
    assert!(loaded
        .iter()
        .all(|op| op.placement == OperationPlacement::Before("llm".into())));
    assert_eq!(
        names(&place_operations(base_steps(), loaded)?),
        [
            "slash_command",
            "tool_execution",
            "stop_hook",
            "max_turns",
            "js:second",
            "js:first",
            "llm"
        ]
    );
    config.set_param(
        "GOOSE_JS_OPERATIONS",
        json!([
            { "path": second, "before": "stop_hook" },
            { "path": first, "after": "js:second" }
        ]),
    )?;
    let loaded = load_operations(&config).await?;
    assert_eq!(
        loaded[0].placement,
        OperationPlacement::Before("stop_hook".into())
    );
    assert_eq!(
        loaded[1].placement,
        OperationPlacement::After("js:second".into())
    );
    config.set_param(
        "GOOSE_JS_OPERATIONS",
        json!([
            { "path": first, "replace": "max_turns" }
        ]),
    )?;
    let loaded = load_operations(&config).await?;
    assert_eq!(
        loaded[0].placement,
        OperationPlacement::Replace("max_turns".into())
    );
    for entry in [
        json!({ "path": first, "before": "llm", "after": "stop_hook" }),
        json!({ "path": first, "before": "llm", "priority": 1 }),
        json!({ "path": first }),
        json!({ "path": first, "before": 42 }),
        json!({ "path": first, "replace": "max_turns", "before": "llm" }),
        json!({ "path": first, "replace": "max_turns", "after": "stop_hook" }),
        json!({ "path": first, "replace": "max_turns", "priority": 1 }),
        json!({ "path": first, "replace": 42 }),
    ] {
        config.set_param("GOOSE_JS_OPERATIONS", [entry])?;
        assert!(load_operations(&config).await.is_err());
    }
    config.set_param("GOOSE_JS_OPERATIONS", [&first, &first])?;
    let Err(error) = load_operations(&config).await else {
        panic!("accepted duplicate names")
    };
    assert!(error
        .to_string()
        .contains("Duplicate JavaScript operation name: js:first"));
    config.set_param("GOOSE_JS_OPERATIONS", 42)?;
    assert!(load_operations(&config).await.is_err());
    config.set_param("GOOSE_JS_OPERATIONS", [directory.path().join("missing.js")])?;
    let Err(error) = load_operations(&config).await else {
        panic!("accepted missing file")
    };
    assert!(format!("{error:#}").contains("missing.js"));
    Ok(())
}

struct NamedOperation(String);

#[async_trait::async_trait]
impl Operation<Session, GooseEffect> for NamedOperation {
    fn name(&self) -> &str {
        &self.0
    }
}

#[async_trait::async_trait]
impl Inference<Session, GooseEffect> for NamedOperation {
    fn applies(&self, _conversation: &goose::conversation::Conversation) -> bool {
        false
    }

    async fn infer(
        &self,
        _session: &Session,
        _conversation: &goose::conversation::Conversation,
        _input: InferenceInput,
        _emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        Ok(OperationResult::NotApplicable)
    }
}

fn base_steps() -> Vec<Step<'static, Session, GooseEffect>> {
    ["slash_command", "tool_execution", "stop_hook", "max_turns"]
        .into_iter()
        .map(|name| {
            Step::Operation(
                Arc::new(NamedOperation(name.into())) as Arc<dyn Operation<Session, GooseEffect>>
            )
        })
        .chain(std::iter::once(Step::Inference(Arc::new(NamedOperation(
            "llm".into(),
        )))))
        .collect()
}

fn names(steps: &[Step<'_, Session, GooseEffect>]) -> Vec<String> {
    steps
        .iter()
        .map(|step| match step {
            Step::Operation(operation) => operation.name().to_owned(),
            Step::Inference(inference) => inference.name().to_owned(),
        })
        .collect()
}

async fn placed(
    name: &str,
    placement: OperationPlacement,
) -> Result<(TempDir, ConfiguredOperation)> {
    let (directory, operation) =
        operation(&format!("const operation = {{ name: {name:?} }};")).await?;
    Ok((
        directory,
        ConfiguredOperation {
            operation: Arc::new(operation),
            placement,
        },
    ))
}

#[tokio::test]
async fn placements_cover_both_ends_and_builtin_boundaries() -> Result<()> {
    let (_first_dir, first) =
        placed("first", OperationPlacement::Before("slash_command".into())).await?;
    let (_tool_dir, tool) =
        placed("tool", OperationPlacement::After("tool_execution".into())).await?;
    let (_stop_dir, stop) = placed("stop", OperationPlacement::Before("stop_hook".into())).await?;
    let (_last_dir, last) = placed("last", OperationPlacement::After("llm".into())).await?;
    let ordered = place_operations(base_steps(), vec![first, tool, stop, last])?;
    assert_eq!(
        names(&ordered),
        [
            "js:first",
            "slash_command",
            "tool_execution",
            "js:tool",
            "js:stop",
            "stop_hook",
            "max_turns",
            "llm",
            "js:last"
        ]
    );
    Ok(())
}

#[tokio::test]
async fn placements_resolve_forward_references_and_preserve_sibling_order() -> Result<()> {
    let (_a_dir, a) = placed("a", OperationPlacement::Before("js:b".into())).await?;
    let (_c_dir, c) = placed("c", OperationPlacement::After("js:b".into())).await?;
    let (_b_dir, b) = placed("b", OperationPlacement::Before("stop_hook".into())).await?;
    let (_d_dir, d) = placed("d", OperationPlacement::Before("stop_hook".into())).await?;
    let (_e_dir, e) = placed("e", OperationPlacement::After("tool_execution".into())).await?;
    let (_f_dir, f) = placed("f", OperationPlacement::After("tool_execution".into())).await?;
    let ordered = place_operations(base_steps(), vec![a, c, b, d, e, f])?;
    assert_eq!(
        names(&ordered),
        [
            "slash_command",
            "tool_execution",
            "js:e",
            "js:f",
            "js:a",
            "js:b",
            "js:c",
            "js:d",
            "stop_hook",
            "max_turns",
            "llm"
        ]
    );
    Ok(())
}

#[tokio::test]
async fn invalid_anchors_and_placement_cycles_fail_setup() -> Result<()> {
    let (_directory, unknown) =
        placed("unknown", OperationPlacement::Before("compaction".into())).await?;
    let Err(error) = place_operations(base_steps(), vec![unknown]) else {
        panic!("accepted missing anchor")
    };
    let error = error.to_string();
    assert!(
        error.contains("js:unknown")
            && error.contains("compaction")
            && error.contains("available anchors")
    );

    let (_a_dir, a) = placed("a", OperationPlacement::Before("js:b".into())).await?;
    let (_b_dir, b) = placed("b", OperationPlacement::After("js:a".into())).await?;
    let Err(error) = place_operations(base_steps(), vec![a, b]) else {
        panic!("accepted cycle")
    };
    assert!(error
        .to_string()
        .contains("Cyclic JavaScript operation placement"));

    let (_self_dir, itself) = placed("self", OperationPlacement::After("js:self".into())).await?;
    assert!(place_operations(base_steps(), vec![itself]).is_err());
    Ok(())
}

#[tokio::test]
async fn replacement_keeps_both_anchors_and_shared_anchor_order() -> Result<()> {
    let (_before_old_dir, before_old) =
        placed("before-old", OperationPlacement::Before("max_turns".into())).await?;
    let (_after_new_dir, after_new) =
        placed("after-new", OperationPlacement::After("js:budget".into())).await?;
    let (_replacement_dir, replacement) =
        placed("budget", OperationPlacement::Replace("max_turns".into())).await?;
    let (_before_new_dir, before_new) =
        placed("before-new", OperationPlacement::Before("js:budget".into())).await?;
    let (_after_old_dir, after_old) =
        placed("after-old", OperationPlacement::After("max_turns".into())).await?;
    let ordered = place_operations(
        base_steps(),
        vec![before_old, after_new, replacement, before_new, after_old],
    )?;
    assert_eq!(
        names(&ordered),
        [
            "slash_command",
            "tool_execution",
            "stop_hook",
            "js:before-old",
            "js:before-new",
            "js:budget",
            "js:after-new",
            "js:after-old",
            "llm"
        ]
    );
    Ok(())
}

#[tokio::test]
async fn conflicting_unknown_js_and_inference_replacements_fail_setup() -> Result<()> {
    for (target, expected) in [
        ("llm", "cannot replace inference operation"),
        (
            "compaction",
            "unknown built-in operation replacement target",
        ),
        ("js:custom", "replace requires a built-in Operation"),
    ] {
        let (_directory, replacement) =
            placed("replacement", OperationPlacement::Replace(target.into())).await?;
        let Err(error) = place_operations(base_steps(), vec![replacement]) else {
            panic!("accepted replacement of {target}")
        };
        assert!(error.to_string().contains(expected));
    }
    let (_first_dir, first) =
        placed("first", OperationPlacement::Replace("max_turns".into())).await?;
    let (_second_dir, second) =
        placed("second", OperationPlacement::Replace("max_turns".into())).await?;
    let Err(error) = place_operations(base_steps(), vec![first, second]) else {
        panic!("accepted conflicting replacements")
    };
    assert!(error
        .to_string()
        .contains("Multiple JavaScript Operations replace"));
    Ok(())
}

struct TrackedBuiltin(Arc<AtomicUsize>);

#[async_trait::async_trait]
impl Operation<Session, GooseEffect> for TrackedBuiltin {
    fn name(&self) -> &str {
        "max_turns"
    }

    async fn prompt_parts(
        &self,
        _session: &Session,
        _conversation: &goose::conversation::Conversation,
    ) -> Result<Vec<(String, String)>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(vec![("builtin".into(), "must not contribute".into())])
    }

    async fn run(
        &self,
        _session: &Session,
        _conversation: &goose::conversation::Conversation,
        _emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(OperationResult::NotApplicable)
    }

    async fn finalize_cancellation(
        &self,
        _session: &Session,
        _conversation: &goose::conversation::Conversation,
        _emit: &Emitter,
    ) -> Vec<GooseEffect> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Vec::new()
    }
}

#[tokio::test]
async fn replacement_removes_builtin_hooks_and_uses_js_identity_for_effects() -> Result<()> {
    let (_session_dir, manager, session) = session().await?;
    let (_operation_dir, replacement) = operation(r#"
        const operation = {
            name: "budget",
            prompt_parts() { return [["budget", "replacement active"]]; },
            run() { return { kind: "applied", status: "yielded" }; },
            finalize_cancellation({ conversation }) {
                return [{ type: "set_message_operation_note", message_id: conversation.messages[0].id,
                    key: "cancelled", value: true }];
            }
        };
    "#).await?;
    let calls = Arc::new(AtomicUsize::new(0));
    let steps = place_operations(
        vec![Step::Operation(Arc::new(TrackedBuiltin(calls.clone())))],
        vec![ConfiguredOperation {
            operation: Arc::new(replacement),
            placement: OperationPlacement::Replace("max_turns".into()),
        }],
    )?;
    let cancel = CancellationToken::new();
    let machine = StateMachine::new(steps, cancel.clone());
    let input = machine
        .collect_inference_input(&session, session.conversation.as_ref().unwrap())
        .await?;
    assert_eq!(
        input.prompt_parts,
        [("budget".to_owned(), "replacement active".to_owned())]
    );
    let (tx, _rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, cancel.clone());
    assert_eq!(
        machine.step(&session, &emit).await?.applied_step.as_deref(),
        Some("js:budget")
    );
    cancel.cancel();
    let saved = machine.finalize(&manager, &session.id, &emit).await?;
    assert_eq!(
        saved.conversation.as_ref().unwrap().messages()[0]
            .metadata
            .operation_note("js:budget", "cancelled"),
        Some(&json!(true))
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn replacing_slash_dispatcher_preserves_its_anchor() -> Result<()> {
    let (_replacement_dir, replacement) = placed(
        "commands",
        OperationPlacement::Replace("slash_command".into()),
    )
    .await?;
    let (_insert_dir, insertion) =
        placed("first", OperationPlacement::Before("slash_command".into())).await?;
    let ordered = place_operations(base_steps(), vec![insertion, replacement])?;
    assert_eq!(
        names(&ordered),
        [
            "js:first",
            "js:commands",
            "tool_execution",
            "stop_hook",
            "max_turns",
            "llm"
        ]
    );
    Ok(())
}

#[tokio::test]
async fn moving_an_operation_before_a_yielding_step_changes_which_step_runs() -> Result<()> {
    let (_session_dir, _manager, session) = session().await?;
    let (_yield_dir, yielding) = operation(r#"
        const operation = { name: "yield", run() { return { kind: "applied", status: "yielded" }; } };
    "#).await?;
    let (_custom_dir, custom) = operation(r#"
        const operation = { name: "custom", run() { return { kind: "applied", status: "yielded" }; } };
    "#).await?;
    let yielding = Arc::new(yielding);
    let custom = Arc::new(custom);
    let (tx, _rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, CancellationToken::new());
    for (placement, expected) in [
        (OperationPlacement::After("js:yield".into()), "js:yield"),
        (OperationPlacement::Before("js:yield".into()), "js:custom"),
    ] {
        let ordered = place_operations(
            vec![Step::Operation(yielding.clone())],
            vec![ConfiguredOperation {
                operation: custom.clone(),
                placement,
            }],
        )?;
        let machine = StateMachine::new(ordered, CancellationToken::new());
        assert_eq!(
            machine.step(&session, &emit).await?.applied_step.as_deref(),
            Some(expected)
        );
    }
    Ok(())
}

#[tokio::test]
async fn effects_persist_and_survive_reconstruction_with_matching_event_ids() -> Result<()> {
    let (_session_dir, manager, session) = session().await?;
    let source = format!(
        r#"const operation = {{
            name: "once",
            async run({{ operation_name, conversation }}) {{
                const first = conversation.messages[0];
                if (first.metadata.operations?.[operation_name]?.handled) {{
                    return {{ kind: "not_applicable" }};
                }}
                console.log("this is separate from the result");
                return {{
                    kind: "applied", status: "continuing", effects: [
                        {{ type: "append_message", message: {} }},
                        {{ type: "set_message_operation_note", message_id: first.id, key: "handled", value: true }}
                    ]
                }};
            }}
        }};"#,
        serde_json::to_string(&Message::assistant().with_text("handled"))?
    );
    let (operation_dir, operation) = operation(&source).await?;
    let machine = StateMachine::new(
        vec![Step::Operation(Arc::new(operation))],
        CancellationToken::new(),
    );
    let (tx, mut rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, CancellationToken::new());
    let mut result = machine.step(&session, &emit).await?;
    assert_eq!(result.applied_step.as_deref(), Some("js:once"));
    assert_eq!(result.status, RunStatus::Continuing);
    machine
        .apply(&manager, &session, &mut result.effects, &emit)
        .await?;
    let saved = manager.get_session(&session.id, true).await?;
    let messages = saved.conversation.as_ref().unwrap().messages();
    assert_eq!(messages.len(), 2);
    assert_eq!(
        messages[0].metadata.operation_note("js:once", "handled"),
        Some(&json!(true))
    );
    let AgentEvent::Message(event) = rx.try_recv()? else {
        panic!("expected message event")
    };
    assert!(event.id.is_some());
    assert_eq!(event.id, messages[1].id);
    assert!(rx.try_recv().is_err());

    let reloaded =
        JavaScriptOperation::from_file(operation_dir.path().join("operation.js")).await?;
    assert!(matches!(
        reloaded
            .run(&saved, saved.conversation.as_ref().unwrap(), &emit)
            .await?,
        OperationResult::NotApplicable
    ));
    Ok(())
}

#[tokio::test]
async fn optional_hooks_default_and_contexts_are_fresh() -> Result<()> {
    let (_session_dir, _manager, session) = session().await?;
    let conversation = session.conversation.as_ref().unwrap();
    let (_operation_dir, operation) = operation(r#"
        let calls = 0;
        const operation = {
            name: "isolated",
            async prompt_parts({ session, conversation }) {
                calls++;
                conversation.messages.length = 0;
                session.name = "changed";
                return [["calls", String(calls)], ["capabilities",
                    [typeof tools, typeof fetch, typeof process, typeof require, typeof setTimeout].join(",")]];
            }
        };
    "#).await?;
    let expected: Vec<(String, String)> = vec![
        ("calls".into(), "1".into()),
        (
            "capabilities".into(),
            "undefined,undefined,undefined,undefined,undefined".into(),
        ),
    ];
    assert_eq!(
        operation.prompt_parts(&session, conversation).await?,
        expected
    );
    assert_eq!(
        operation.prompt_parts(&session, conversation).await?,
        expected
    );
    assert_eq!(conversation.messages().len(), 1);
    assert_eq!(session.name, "JavaScript operation");
    assert!(operation.inference_tools(&session).await?.is_empty());
    assert!(operation
        .moim_parts(&session, conversation)
        .await?
        .is_empty());
    let (tx, _rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, CancellationToken::new());
    assert!(matches!(
        operation.run(&session, conversation, &emit).await?,
        OperationResult::NotApplicable
    ));
    assert!(matches!(
        operation
            .run_command(
                &SlashCommand {
                    command: "test",
                    params_str: ""
                },
                &session,
                conversation,
                &emit
            )
            .await?,
        OperationResult::NotApplicable
    ));
    assert!(operation
        .finalize_cancellation(&session, conversation, &emit)
        .await
        .is_empty());
    Ok(())
}

#[tokio::test]
async fn command_and_inference_contributions_are_typed() -> Result<()> {
    let (_session_dir, _manager, session) = session().await?;
    let tool = Tool::new(
        "local_tool",
        "A local tool",
        json!({ "type": "object" }).as_object().unwrap().clone(),
    );
    let (_operation_dir, operation) = operation(&format!(r#"
        const operation = {{
            name: "hooks",
            inference_tools() {{ return [{}]; }},
            moim_parts({{ conversation }}) {{ return ["messages: " + conversation.messages.length]; }},
            async run_command({{ command }}) {{
                if (command.command !== "local" || command.params_str !== "hello") {{
                    return {{ kind: "not_applicable" }};
                }}
                return {{ kind: "applied", status: "yielded", effects: [] }};
            }}
        }};
    "#, serde_json::to_string(&tool)?)).await?;
    assert_eq!(
        operation.inference_tools(&session).await?[0].name,
        "local_tool"
    );
    let conversation = session.conversation.as_ref().unwrap();
    assert_eq!(
        operation.moim_parts(&session, conversation).await?,
        ["messages: 1"]
    );
    let (tx, _rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, CancellationToken::new());
    let OperationResult::Applied(result) = operation
        .run_command(
            &SlashCommand {
                command: "local",
                params_str: "hello",
            },
            &session,
            conversation,
            &emit,
        )
        .await?
    else {
        panic!("expected command to apply")
    };
    assert_eq!(result.status, RunStatus::Yielded);
    Ok(())
}

#[tokio::test]
async fn loaded_source_is_pinned_until_reloaded() -> Result<()> {
    let (_session_dir, _manager, session) = session().await?;
    let source =
        r#"const operation = { name: "snapshot", moim_parts() { return ["original"]; } };"#;
    let (directory, original) = operation(source).await?;
    let path = directory.path().join("operation.js");
    tokio::fs::write(&path, source.replace("original", "updated")).await?;
    let conversation = session.conversation.as_ref().unwrap();
    assert_eq!(
        original.moim_parts(&session, conversation).await?,
        ["original"]
    );
    let updated = JavaScriptOperation::from_file(path).await?;
    assert_eq!(
        updated.moim_parts(&session, conversation).await?,
        ["updated"]
    );
    Ok(())
}

#[tokio::test]
async fn invalid_files_and_results_fail_without_emitting_partial_effects() -> Result<()> {
    for source in [
        "const = ;",
        "const operation = null;",
        "const operation = {};",
        "const operation = { name: ' ', run() {} };",
        "const operation = { name: 'bad', run: 42 };",
        "const operation = { name: 'bad', promt_parts() {} };",
    ] {
        let directory = TempDir::new()?;
        let path = directory.path().join("invalid.js");
        tokio::fs::write(&path, source).await?;
        let Err(error) = JavaScriptOperation::from_file(&path).await else {
            panic!("accepted {source}")
        };
        assert!(format!("{error:#}").contains("invalid.js"));
    }
    let (_session_dir, _manager, session) = session().await?;
    let (_directory, invalid) = operation(&format!(
        r#"
        const operation = {{ name: "invalid", run() {{
            return {{ kind: "applied", status: "continuing", effects: [
                {{ type: "append_message", message: {} }},
                {{ type: "unknown_effect" }}
            ] }};
        }} }};
    "#,
        serde_json::to_string(&Message::assistant().with_text("must not emit"))?
    ))
    .await?;
    let (tx, mut rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, CancellationToken::new());
    assert!(invalid
        .run(&session, session.conversation.as_ref().unwrap(), &emit)
        .await
        .is_err());
    assert!(rx.try_recv().is_err());
    Ok(())
}

#[tokio::test]
async fn cancellation_drops_execution_and_finalizes_with_persisted_effects() -> Result<()> {
    let (_session_dir, manager, session) = session().await?;
    let (_operation_dir, operation) = operation(r#"
        const operation = {
            name: "cancel",
            async run() { await new Promise(() => {}); },
            finalize_cancellation({ conversation }) {
                return [{ type: "set_message_operation_note", message_id: conversation.messages[0].id,
                    key: "cancelled", value: true }];
            }
        };
    "#).await?;
    let cancel = CancellationToken::new();
    let machine = StateMachine::new(vec![Step::Operation(Arc::new(operation))], cancel.clone());
    let (tx, _rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, cancel.clone());
    let step = machine.step(&session, &emit);
    tokio::pin!(step);
    tokio::select! {
        result = &mut step => panic!("step finished before cancellation: {:?}", result.err()),
        _ = tokio::time::sleep(Duration::from_millis(50)) => cancel.cancel(),
    }
    assert_eq!(step.await?.status, RunStatus::Cancelled);
    let saved = machine.finalize(&manager, &session.id, &emit).await?;
    assert_eq!(
        saved.conversation.as_ref().unwrap().messages()[0]
            .metadata
            .operation_note("js:cancel", "cancelled"),
        Some(&json!(true))
    );
    Ok(())
}

#[tokio::test]
async fn nonterminating_hooks_are_bounded() -> Result<()> {
    let (_session_dir, _manager, session) = session().await?;
    for body in ["while (true) {}", "await new Promise(() => {})"] {
        let (_directory, operation) = operation(&format!(
            "const operation = {{ name: 'bounded', async run() {{ {body} }} }};"
        ))
        .await?;
        let operation = operation.with_timeout(Duration::from_millis(100));
        let (tx, _rx) = mpsc::unbounded_channel();
        let emit = Emitter::new(tx, CancellationToken::new());
        assert!(operation
            .run(&session, session.conversation.as_ref().unwrap(), &emit)
            .await
            .is_err());
    }
    Ok(())
}

#[tokio::test]
async fn example_file_handles_its_slash_command() -> Result<()> {
    let (_session_dir, _manager, session) = session().await?;
    let operation = JavaScriptOperation::from_file(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("examples/js-operations/local-policy.js"),
    )
    .await?;
    let (tx, _rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, CancellationToken::new());
    let OperationResult::Applied(result) = operation
        .run_command(
            &SlashCommand {
                command: "local-policy",
                params_str: "",
            },
            &session,
            session.conversation.as_ref().unwrap(),
            &emit,
        )
        .await?
    else {
        panic!("example command did not apply")
    };
    assert_eq!(result.status, RunStatus::Yielded);
    assert!(matches!(
        result.effects.as_slice(),
        [GooseEffect::Conversation(
            ConversationEffect::AppendMessage(_)
        )]
    ));
    Ok(())
}
