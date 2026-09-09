mod support;
use eredu_core::{GenerationCancellationToken, GenerationConfigOverrides, ResidencyPlan};
use goose_local_inference::{
    eredu_adapter::{generation::Request, settings::generation_settings, WorkerHandle},
    model::{ChatTemplate, ModelSettings, SamplingConfig, ToolCallingMode},
};
use goose_provider_types::{
    conversation::{
        message::{Message, MessageContent},
        token_usage::ProviderUsage,
    },
    errors::ProviderError,
    model::ModelConfig,
};
use serde_json::json;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use support::{Calls, MockBackend};

fn request() -> Request {
    Request {
        model: ModelConfig::new("fixture"),
        settings: ModelSettings::default(),
        system: "You are Goose".into(),
        messages: vec![Message::user().with_text("hello")],
        tools: vec![],
        message_id: "response-1".into(),
        model_load_ms: Some(7),
    }
}

fn worker(
    output: &str,
    settings: ModelSettings,
    checkpoint: Option<serde_json::Value>,
) -> (tempfile::TempDir, WorkerHandle, Arc<Mutex<Calls>>) {
    let dir = tempfile::tempdir().unwrap();
    support::write_artifact(dir.path());
    if let Some(checkpoint) = checkpoint {
        std::fs::write(
            dir.path().join("generation_config.json"),
            serde_json::to_vec(&checkpoint).unwrap(),
        )
        .unwrap();
    }
    let calls = Arc::new(Mutex::new(Calls::default()));
    let observed = calls.clone();
    let tokens = support::encode(output);
    let worker = WorkerHandle::spawn(
        move || MockBackend::new(observed, tokens),
        dir.path().to_owned(),
        settings,
        None,
    )
    .unwrap();
    (dir, worker, calls)
}

fn run(
    worker: &WorkerHandle,
    request: Request,
) -> (Result<ProviderUsage, ProviderError>, Vec<Message>) {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<
        Result<(Option<Message>, Option<ProviderUsage>), ProviderError>,
    >(2);
    std::thread::scope(|scope| {
        let reader = scope.spawn(move || {
            let mut messages = vec![];
            while let Some(result) = rx.blocking_recv() {
                if let Some(message) = result.unwrap().0 {
                    messages.push(message);
                }
            }
            messages
        });
        let result = worker.generate(request, tx, GenerationCancellationToken::new());
        (result, reader.join().unwrap())
    })
}

fn text(messages: &[Message]) -> String {
    messages
        .iter()
        .flat_map(|message| &message.content)
        .filter_map(|content| match content {
            MessageContent::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn checkpoint_defaults_single_override_and_request_precedence_reach_backend() {
    let checkpoint = json!({"do_sample":true,"temperature":0.6,"top_k":17,"top_p":0.83,"min_p":0.03,"repetition_penalty":1.2,"repeat_last_n":32,"max_new_tokens":30});
    let (_dir, worker, calls) = worker(
        "hello<|im_end|>",
        ModelSettings::default(),
        Some(checkpoint.clone()),
    );
    let (result, messages) = run(&worker, request());
    let usage = result.unwrap();
    assert_eq!(text(&messages), "hello");
    assert_eq!(usage.usage.output_tokens, Some(6)); // Five letters and committed EOS.
    assert!(usage.stats.unwrap().time_to_first_token_ms.is_some());
    let checkpoint = serde_json::from_value(checkpoint).unwrap();
    let expected = eredu_core::resolve_generation_config(
        Some(&checkpoint),
        GenerationConfigOverrides::default(),
    )
    .unwrap();
    assert_eq!(calls.lock().unwrap().configs[0].sampling(), expected);
    let mut req = request();
    req.settings.presence_penalty = Some(0.4);
    req.settings.sampling = SamplingConfig::Temperature {
        temperature: Some(0.2),
        top_k: None,
        top_p: None,
        min_p: None,
        seed: None,
    };
    req.model.temperature = Some(0.9);
    run(&worker, req).0.unwrap();
    let calls = calls.lock().unwrap();
    let actual = calls.configs[1].sampling();
    assert_eq!(actual.temperature, 0.9);
    assert_eq!(actual.top_k, 17);
    assert_eq!(actual.top_p, 0.83);
    assert_eq!(actual.presence_penalty, 0.4);
    assert_eq!(calls.loads, 1);
    assert_eq!(calls.resets, 2);
    assert_ne!(calls.threads[0], std::thread::current().id());
}

#[test]
fn checkpoint_do_sample_and_fallbacks_are_eredus() {
    for checkpoint in [
        None,
        Some(json!({"do_sample":false,"temperature":0.8})),
        Some(json!({"do_sample":true})),
    ] {
        let (_dir, worker, calls) =
            worker("hi<|im_end|>", ModelSettings::default(), checkpoint.clone());
        run(&worker, request()).0.unwrap();
        let checkpoint = checkpoint.map(|value| serde_json::from_value(value).unwrap());
        assert_eq!(
            calls.lock().unwrap().configs[0].sampling(),
            eredu_core::resolve_generation_config(checkpoint.as_ref(), Default::default()).unwrap()
        );
    }
}

#[test]
fn persisted_overrides_are_explicit_and_reset_restores_inheritance() {
    let saved = json!({"backend_id":"eredu","sampling":{"type":"Temperature","temperature":0.8,"top_k":40,"top_p":0.95,"min_p":0.05},"repeat_penalty":1.0,"repeat_last_n":64,"frequency_penalty":0.0,"presence_penalty":0.0});
    let settings: ModelSettings = serde_json::from_value(saved).unwrap();
    let overrides = generation_settings(&settings, &ModelConfig::new("fixture"))
        .unwrap()
        .overrides;
    assert_eq!(overrides.temperature, Some(0.8));
    assert_eq!(overrides.frequency_penalty, Some(0.0));
    let absent: ModelSettings = serde_json::from_value(json!({"backend_id":"eredu"})).unwrap();
    assert_eq!(
        generation_settings(&absent, &ModelConfig::new("fixture"))
            .unwrap()
            .overrides,
        GenerationConfigOverrides::default()
    );
    let encoded = serde_json::to_value(&absent).unwrap();
    let decoded: ModelSettings = serde_json::from_value(encoded).unwrap();
    assert_eq!(
        generation_settings(&decoded, &ModelConfig::new("fixture"))
            .unwrap()
            .overrides,
        GenerationConfigOverrides::default()
    );
    #[cfg(feature = "hf-hub")]
    {
        let dto = goose_local_inference::management::model_settings_to_dto(&decoded);
        let roundtrip = goose_local_inference::management::model_settings_from_dto(
            serde_json::from_value(serde_json::to_value(dto).unwrap()).unwrap(),
        );
        assert_eq!(
            generation_settings(&roundtrip, &ModelConfig::new("fixture"))
                .unwrap()
                .overrides,
            GenerationConfigOverrides::default()
        );
    }
}

#[test]
fn planner_realization_and_constraints_are_retained() {
    for (model_bytes, residency) in [
        (1024, "resident"),
        (16 * 1024, "host"),
        (128 * 1024, "disk"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        support::write_artifact(dir.path());
        let calls = Arc::new(Mutex::new(Calls::default()));
        let observed = calls.clone();
        let settings = ModelSettings {
            max_cached_shards: Some(7),
            ..Default::default()
        };
        let worker = WorkerHandle::spawn(
            move || {
                let mut backend = MockBackend::new(observed, vec![1]);
                backend.model_bytes = model_bytes;
                backend
            },
            dir.path().into(),
            settings,
            None,
        )
        .unwrap();
        assert!(matches!(
            (residency, worker.report.plan.residency()),
            ("resident", ResidencyPlan::FullyResident)
                | ("host", ResidencyPlan::LayerwiseHost { .. })
                | ("disk", ResidencyPlan::DenseDiskStream { .. })
        ));
        assert_eq!(calls.lock().unwrap().realized[0], worker.report.plan);
        assert!(calls
            .lock()
            .unwrap()
            .admitted
            .iter()
            .all(|plan| plan.max_cached_shards() == 7));
        drop(worker);
        assert_eq!(calls.lock().unwrap().drops, 1);
    }
}

#[test]
fn mirostat_is_not_greedy_and_requires_positive_effective_temperature() {
    let (_dir, worker, calls) = worker(
        "ok<|im_end|>",
        ModelSettings::default(),
        Some(json!({"do_sample":false})),
    );
    let mut req = request();
    req.settings.sampling = SamplingConfig::MirostatV2 {
        temperature: None,
        tau: 5.0,
        eta: 0.1,
        seed: Some(42),
    };
    assert!(run(&worker, req.clone())
        .0
        .unwrap_err()
        .to_string()
        .contains("positive effective temperature"));
    req.model.temperature = Some(0.8);
    run(&worker, req).0.unwrap();
    assert!(matches!(
        calls.lock().unwrap().configs[0].strategy(),
        eredu_core::TextSamplingStrategy::MirostatV2 { .. }
    ));
}

#[test]
fn context_admission_caps_generation_and_rejects_oversized_prompts() {
    let (_dir, worker, calls) = worker(&"a".repeat(400), ModelSettings::default(), None);
    let mut req = request();
    req.settings.context_size = Some(1);
    assert!(matches!(
        run(&worker, req).0,
        Err(ProviderError::ContextLengthExceeded(_))
    ));
    assert!(calls.lock().unwrap().configs.is_empty());
    let mut req = request();
    req.model.max_tokens = Some(4);
    let (result, messages) = run(&worker, req);
    assert_eq!(result.unwrap().usage.output_tokens, Some(4));
    assert_eq!(text(&messages), "aaaa");
    let prompt = calls.lock().unwrap().prompts[0].len();
    let mut req = request();
    req.settings.context_size = Some(1);
    req.model.context_limit = Some(prompt + 2);
    let (result, messages) = run(&worker, req);
    assert_eq!(result.unwrap().usage.output_tokens, Some(2));
    assert_eq!(text(&messages), "aa");
}

#[test]
fn unicode_hidden_reasoning_stop_sequences_and_eos_stream_once() {
    let (_dir, worker, _) = worker(
        "<think>\nsecret\n</think>\n\nhi😀STOPnever<|im_end|>",
        ModelSettings {
            chat_template: ChatTemplate::CustomInline {
                template: include_str!("support/qwen3.jinja").into(),
            },
            ..Default::default()
        },
        None,
    );
    let mut req = request();
    req.model.request_params = Some([("stop".into(), json!(["STOP"]))].into());
    let (result, messages) = run(&worker, req);
    result.unwrap();
    assert_eq!(text(&messages), "\n\nhi😀");
    let thinking: String = messages
        .iter()
        .flat_map(|message| &message.content)
        .filter_map(|content| match content {
            MessageContent::Thinking(thinking) => Some(thinking.thinking.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(thinking.trim(), "secret");
    assert!(messages
        .iter()
        .all(|message| message.id.as_deref() == Some("response-1")));
}

#[test]
fn native_tools_validate_completed_arguments_and_preserve_schema_keywords() {
    let schema = json!({"type":"object","properties":{"count":{"type":"integer","minimum":1},"optional":{"type":["string","null"]},"nested":{"anyOf":[{"type":"array","items":{"type":"object","properties":{"x":{"type":"number"}},"required":["x"]}},{"type":"null"}]}},"required":["count"]});
    for (arguments, valid) in [
        (r#"{"count":2,"optional":null,"nested":[{"x":1.5}]}"#, true),
        (r#"{"count":2,"optional":"value","nested":null}"#, true),
        (r#"{"count":2,"nested":[{}]}"#, false),
        (r#"{"count":2,"nested":[{"x":"wrong type"}]}"#, false),
        (r#"{"count":2,"nested":{}}"#, false),
        (r#"{"count":2,"optional":42}"#, false),
        ("{\"count\":0}", false),
        ("{\"count\":", false),
    ] {
        let output=format!("<tool_call>\n{{\"name\":\"delegate\",\"arguments\":{arguments}}}\n</tool_call><|im_end|>");
        let (_dir, worker, _) = worker(&output, ModelSettings::default(), None);
        let mut req = request();
        req.settings.tool_calling = ToolCallingMode::ForceNative;
        req.tools = vec![rmcp::model::Tool::new(
            "delegate",
            "delegate work",
            schema.as_object().unwrap().clone(),
        )];
        let (result, messages) = run(&worker, req);
        let executable: Vec<_> = messages
            .iter()
            .flat_map(|message| &message.content)
            .filter_map(|content| match content {
                MessageContent::ToolRequest(call) => Some(call),
                _ => None,
            })
            .collect();
        if valid {
            result.unwrap();
            assert_eq!(executable.len(), 1, "arguments: {arguments}");
            let call = executable[0].tool_call.as_ref().unwrap();
            assert_eq!(call.name, "delegate");
            assert_eq!(
                serde_json::to_value(call.arguments.as_ref().unwrap()).unwrap(),
                serde_json::from_str::<serde_json::Value>(arguments).unwrap()
            );
        } else {
            assert!(executable.is_empty(), "arguments: {arguments}");
            assert!(result.is_err(), "arguments: {arguments}");
        }
    }
}

#[test]
fn custom_template_inspection_and_loading_agree_and_force_native_fails_before_generation() {
    let settings = ModelSettings {
        chat_template: ChatTemplate::CustomInline {
            template: "{% for m in messages %}{{ m.content }}{% endfor %}assistant:".into(),
        },
        ..Default::default()
    };
    let (_dir, worker, calls) = worker("ok<|im_end|>", settings.clone(), None);
    assert_eq!(
        worker.inspection.chat_template,
        eredu_core::InspectionReadiness::Ready
    );
    let mut req = request();
    req.settings = settings;
    req.settings.tool_calling = ToolCallingMode::ForceNative;
    req.tools = vec![rmcp::model::Tool::new(
        "tool",
        "tool",
        json!({"type":"object"}).as_object().unwrap().clone(),
    )];
    assert!(run(&worker, req)
        .0
        .unwrap_err()
        .to_string()
        .contains("Native tool calling"));
    assert!(calls.lock().unwrap().configs.is_empty());
}

#[test]
fn cancellation_settles_and_reuses_the_same_thread_bound_model() {
    let dir = tempfile::tempdir().unwrap();
    support::write_artifact(dir.path());
    let calls = Arc::new(Mutex::new(Calls::default()));
    let observed = calls.clone();
    let worker = WorkerHandle::spawn(
        move || {
            let mut b = MockBackend::new(observed, support::encode(&"x".repeat(1000)));
            b.delay = true;
            b
        },
        dir.path().into(),
        Default::default(),
        None,
    )
    .unwrap();
    let token = GenerationCancellationToken::new();
    let cancel = token.clone();
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    std::thread::scope(|scope| {
        scope.spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            cancel.cancel();
            while rx.blocking_recv().is_some() {}
        });
        let usage = worker.generate(request(), tx, token).unwrap();
        assert_eq!(usage.finish_reasons.unwrap(), vec!["cancelled"]);
    });
    let mut req = request();
    req.model.max_tokens = Some(1);
    run(&worker, req).0.unwrap();
    drop(worker);
    let calls = calls.lock().unwrap();
    assert_eq!(calls.loads, 1);
    assert_eq!(calls.resets, 2);
    assert_eq!(calls.drops, 1);
}

#[test]
fn unrecognized_template_reports_timing_and_committed_counts_with_stops() {
    let settings = ModelSettings {
        chat_template: ChatTemplate::CustomInline {
            template: include_str!("support/plain.jinja").into(),
        },
        ..Default::default()
    };
    for (stop, reason) in [("STOP", "stop_sequence"), ("<|im_end|>", "eos")] {
        let output = format!("hi😀{stop}never");
        let (_dir, worker, _) = worker(&output, settings.clone(), None);
        let mut req = request();
        req.settings = settings.clone();
        req.model.request_params = Some([("stop".into(), json!(["STOP"]))].into());
        let (usage, messages) = run(&worker, req);
        let usage = usage.unwrap();
        assert_eq!(text(&messages), "hi😀");
        assert_eq!(
            usage.usage.output_tokens,
            Some(support::encode(&format!("hi😀{stop}")).len() as i32)
        );
        assert!(usage.stats.unwrap().time_to_first_token_ms.is_some());
        assert_eq!(usage.finish_reasons.unwrap(), vec![reason]);
    }
}

#[test]
fn prepared_speculation_streams_committed_output_and_maps_statistics() {
    for template in [
        ChatTemplate::default(),
        ChatTemplate::CustomInline {
            template: include_str!("support/plain.jinja").into(),
        },
    ] {
        for sampling in [
            SamplingConfig::Inherit,
            SamplingConfig::MirostatV2 {
                temperature: Some(0.8),
                tau: 5.0,
                eta: 0.1,
                seed: Some(42),
            },
        ] {
            let dir = tempfile::tempdir().unwrap();
            support::write_artifact(dir.path());
            let calls = Arc::new(Mutex::new(Calls::default()));
            let observed = calls.clone();
            let settings = ModelSettings {
                chat_template: template.clone(),
                ..Default::default()
            };
            let worker = WorkerHandle::spawn(
                move || {
                    let mut b = MockBackend::new(observed, vec![]);
                    b.embedded_draft = true;
                    b
                },
                dir.path().into(),
                settings.clone(),
                None,
            )
            .unwrap();
            assert!(matches!(
                worker.report.plan.drafting(),
                eredu_core::DraftingPlan::Embedded {
                    max_draft_tokens: 2,
                    ..
                }
            ));
            let mut req = request();
            req.settings = settings;
            req.settings.sampling = sampling;
            req.model.max_tokens = Some(8);
            let (usage, messages) = run(&worker, req);
            let usage = usage.unwrap();
            assert_eq!(text(&messages), "abc");
            assert_eq!(usage.usage.output_tokens, Some(4));
            let stats = usage.stats.unwrap();
            let draft = stats.draft.unwrap();
            assert_eq!(draft.draft_tokens, 4);
            assert_eq!(draft.accepted_tokens, 1);
            assert_eq!(
                draft.target_tokens,
                usage.usage.input_tokens.unwrap() as usize + 6
            );
            assert_eq!(draft.accept_rate, 0.25);
            assert_eq!(draft.rounds, 2);
            assert!(stats.time_to_first_token_ms.is_some());
            assert_eq!(calls.lock().unwrap().configs.len(), 1);
        }
    }
}

#[test]
fn explicit_device_and_accelerator_then_cpu_policy() {
    use eredu_core::AutomaticPlanningBackend;
    let backend = MockBackend::new(Arc::default(), vec![]);
    let mut hardware = backend.discover_hardware().unwrap();
    let mut cpu = hardware.backends[0].devices[0].clone();
    cpu.family = "cpu".into();
    cpu.id = "cpu:0".into();
    hardware.backends[0].devices.insert(0, cpu);
    let select = goose_local_inference::eredu_adapter::planning::select_device;
    assert_eq!(
        select(&hardware, &backend.backend_id(), None)
            .unwrap()
            .device(),
        "gpu:0"
    );
    assert_eq!(
        select(&hardware, &backend.backend_id(), Some("cpu:0"))
            .unwrap()
            .device(),
        "cpu:0"
    );
    assert!(select(&hardware, &backend.backend_id(), Some("gpu:9")).is_err());
    hardware.backends[0].devices.pop();
    assert_eq!(
        select(&hardware, &backend.backend_id(), None)
            .unwrap()
            .device(),
        "cpu:0"
    );
}

#[test]
fn failed_settlement_and_panicked_workers_cannot_be_reused() {
    for panic in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        support::write_artifact(dir.path());
        let calls = Arc::new(Mutex::new(Calls::default()));
        let observed = calls.clone();
        let worker = WorkerHandle::spawn(
            move || {
                let b = MockBackend::new(observed, support::encode("hi<|im_end|>"));
                if panic {
                    b.panic_generate
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                } else {
                    b.fail_settle
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                }
                b
            },
            dir.path().into(),
            Default::default(),
            None,
        )
        .unwrap();
        assert!(run(&worker, request()).0.is_err());
        assert!(run(&worker, request()).0.is_err());
        drop(worker);
        assert_eq!(calls.lock().unwrap().drops, 1);
    }
}

fn tools() -> Vec<rmcp::model::Tool> {
    let mut tool = rmcp::model::Tool::new(
        "lookup",
        "lookup",
        json!({"type":"object","properties":{"x":{"type":"integer","minimum":1}},"required":["x"]})
            .as_object()
            .unwrap()
            .clone(),
    );
    tool.description = None;
    vec![tool]
}

#[test]
fn auto_native_preserves_parallel_calls_and_multiturn_results() {
    let output = "<tool_call>\n{\"name\":\"lookup\",\"arguments\":{\"x\":1}}\n</tool_call>\n<tool_call>\n{\"name\":\"lookup\",\"arguments\":{\"x\":2}}\n</tool_call><|im_end|>";
    let (_dir, worker, calls) = worker(output, ModelSettings::default(), None);
    let mut req = request();
    req.tools = tools();
    req.messages.push(
        Message::user()
            .with_text("invisible-marker")
            .with_visibility(true, false),
    );
    let (usage, messages) = run(&worker, req.clone());
    let usage = usage.unwrap();
    assert_eq!(
        usage.usage.output_tokens,
        Some(support::encode(output).len() as i32)
    );
    let requests: Vec<_> = messages
        .iter()
        .flat_map(|m| &m.content)
        .filter_map(|c| match c {
            MessageContent::ToolRequest(call) => Some(call.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 2);
    assert_ne!(requests[0].id, requests[1].id);
    assert_eq!(
        requests[0]
            .tool_call
            .as_ref()
            .unwrap()
            .arguments
            .as_ref()
            .unwrap()["x"],
        1
    );
    assert_eq!(
        requests[1]
            .tool_call
            .as_ref()
            .unwrap()
            .arguments
            .as_ref()
            .unwrap()["x"],
        2
    );
    req.messages.extend(messages);
    for call in requests {
        req.messages.push(Message::user().with_tool_response(
            call.id,
            Ok(rmcp::model::CallToolResult::success(vec![
                rmcp::model::ContentBlock::text("tool-result-marker"),
            ])),
        ));
    }
    run(&worker, req).0.unwrap();
    let calls = calls.lock().unwrap();
    let prompt = support::tokenizer()
        .decode(&calls.prompts[1], false)
        .unwrap();
    assert!(!prompt.contains("invisible-marker"));
    assert!(prompt.contains("tool-result-marker"));
    assert!(prompt.contains("\"x\": 1") || prompt.contains("\"x\":1"));
    assert!(prompt.contains("\"x\": 2") || prompt.contains("\"x\":2"));
    assert!(prompt.contains("You are Goose"));
}

#[test]
fn auto_fallback_and_force_emulated_choose_once_before_generation() {
    for (mode, chat_template) in [
        (
            ToolCallingMode::Auto,
            ChatTemplate::CustomInline {
                template: include_str!("support/plain.jinja").into(),
            },
        ),
        (ToolCallingMode::ForceEmulated, ChatTemplate::default()),
    ] {
        let settings = ModelSettings {
            tool_calling: mode,
            chat_template,
            ..Default::default()
        };
        let (_dir, worker, calls) = worker(
            "$ pwd\nthis must not execute<|im_end|>",
            settings.clone(),
            None,
        );
        let mut req = request();
        req.settings = settings;
        req.tools = tools();
        let (usage, messages) = run(&worker, req.clone());
        usage.unwrap();
        let requests: Vec<_> = messages
            .iter()
            .flat_map(|m| &m.content)
            .filter_map(|c| match c {
                MessageContent::ToolRequest(call) => Some(call),
                _ => None,
            })
            .collect();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].tool_call.as_ref().unwrap().name,
            "developer__shell"
        );
        assert!(!text(&messages).contains("must not execute"));
        assert_eq!(calls.lock().unwrap().configs.len(), 1);
        req.messages.extend(messages.clone());
        req.messages.push(Message::user().with_tool_response(
            requests[0].id.clone(),
            Ok(rmcp::model::CallToolResult::success(vec![
                rmcp::model::ContentBlock::text("directory-result"),
            ])),
        ));
        run(&worker, req).0.unwrap();
        let prompt = support::tokenizer()
            .decode(&calls.lock().unwrap().prompts[1], false)
            .unwrap();
        assert!(prompt.contains("$ pwd"));
        assert!(prompt.contains("Command output:\ndirectory-result"));
    }
}

#[test]
fn invalid_schema_and_cancelled_arguments_never_fallback_or_execute() {
    let (_dir, worker, calls) = worker("ok<|im_end|>", Default::default(), None);
    let mut req = request();
    req.tools = vec![rmcp::model::Tool::new(
        "bad",
        "bad",
        json!({"type":"object","properties":{"x":{"type":"not-a-type"}}})
            .as_object()
            .unwrap()
            .clone(),
    )];
    assert!(run(&worker, req).0.is_err());
    assert!(calls.lock().unwrap().configs.is_empty());
    let request = request();
    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
    let token = GenerationCancellationToken::new();
    let mut emitter = goose_local_inference::eredu_adapter::generation::Emitter::new(
        &request,
        &tx,
        token.clone(),
        false,
    );
    emitter.push(eredu_core::SemanticEvent::ToolCallStart {
        index: 0,
        id: "stable".into(),
        name: "lookup".into(),
    });
    emitter.push(eredu_core::SemanticEvent::ToolArgumentsDelta {
        index: 0,
        json_fragment: "{\"x\":1}".into(),
    });
    token.cancel();
    emitter.push(eredu_core::SemanticEvent::ToolCallEnd);
    assert!(rx.try_recv().is_err());
}

#[test]
fn named_checkpoint_template_and_explicit_builtin_have_effective_inspection() {
    let dir = tempfile::tempdir().unwrap();
    support::write_artifact(dir.path());
    let tokenizer_config = dir.path().join("tokenizer_config.json");
    let mut config = json!({});
    std::fs::remove_file(dir.path().join("chat_template.jinja")).unwrap();
    config["chat_template"] = json!([
        {"name":"default","template":"{% for m in messages %}{{ m.content }}{% endfor %}named-default:"},
        {"name":"tool_use","template":support::TEMPLATE}
    ]);
    std::fs::write(tokenizer_config, serde_json::to_vec(&config).unwrap()).unwrap();
    for template in [
        ChatTemplate::Embedded,
        ChatTemplate::Builtin {
            name: "chatml".into(),
        },
    ] {
        let calls = Arc::new(Mutex::new(Calls::default()));
        let observed = calls.clone();
        let worker = WorkerHandle::spawn(
            move || MockBackend::new(observed, support::encode("hi<|im_end|>")),
            dir.path().into(),
            ModelSettings {
                chat_template: template.clone(),
                ..Default::default()
            },
            None,
        )
        .unwrap();
        assert_eq!(
            worker.inspection.chat_template,
            eredu_core::InspectionReadiness::Ready
        );
        run(&worker, request()).0.unwrap();
        let prompt = support::tokenizer()
            .decode(&calls.lock().unwrap().prompts[0], false)
            .unwrap();
        assert_eq!(
            prompt.contains("named-default:"),
            template == ChatTemplate::Embedded
        );
    }
}

#[test]
fn semantic_delivery_preserves_multiple_calls_and_finishes_once() {
    use eredu_core::{FinishReason, SemanticEvent};
    let req = request();
    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
    let mut emitter = goose_local_inference::eredu_adapter::generation::Emitter::new(
        &req,
        &tx,
        GenerationCancellationToken::new(),
        false,
    );
    for index in 0..2 {
        emitter.push(SemanticEvent::ToolCallStart {
            index,
            id: format!("call_{index}"),
            name: "lookup".into(),
        });
        emitter.push(SemanticEvent::ToolArgumentsDelta {
            index,
            json_fragment: "{\"x\":".into(),
        });
        assert!(rx.try_recv().is_err());
        emitter.push(SemanticEvent::ToolArgumentsDelta {
            index,
            json_fragment: "1}".into(),
        });
        assert!(rx.try_recv().is_err());
        emitter.push(SemanticEvent::ToolCallEnd);
        let (message, usage) = rx.try_recv().unwrap().unwrap();
        assert!(usage.is_none());
        let message = message.unwrap();
        let MessageContent::ToolRequest(call) = &message.content[0] else {
            panic!("expected a completed call")
        };
        assert_eq!(call.id, format!("response-1_call_{index}"));
    }
    emitter.push(SemanticEvent::Finished {
        reason: FinishReason::Eos,
    });
    emitter.push(SemanticEvent::TextDelta("duplicate".into()));
    assert!(rx.try_recv().is_err());
}

#[test]
fn generic_thinking_off_reaches_the_effective_template() {
    let settings = ModelSettings {
        chat_template: ChatTemplate::CustomInline {
            template: include_str!("support/qwen3.jinja").into(),
        },
        ..Default::default()
    };
    let (_dir, worker, calls) = worker("hi<|im_end|>", settings, None);
    let mut req = request();
    req.model = req
        .model
        .with_thinking_effort(goose_provider_types::thinking::ThinkingEffort::Off);
    let (usage, messages) = run(&worker, req);
    usage.unwrap();
    assert_eq!(text(&messages), "hi");
    let prompt = support::tokenizer()
        .decode(&calls.lock().unwrap().prompts[0], false)
        .unwrap();
    assert!(prompt.ends_with("<think>\n\n</think>\n\n"));
}
