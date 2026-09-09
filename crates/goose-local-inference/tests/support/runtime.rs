// Included as a crate test to exercise private runtime scheduling with a thread-bound backend.
use super::*;
use futures::StreamExt;
#[path = "mod.rs"]
mod support;
use support::{Calls, MockBackend};

struct MockFactory {
    calls: Arc<StdMutex<Calls>>,
}
impl LocalInferenceBackend for MockFactory {
    fn id(&self) -> &'static str {
        EREDU_BACKEND_ID
    }
    fn load_model(
        &self,
        _: &str,
        resolved: &ResolvedModelPaths,
        settings: &model::ModelSettings,
    ) -> Result<Box<dyn BackendLoadedModel>, ProviderError> {
        let calls = self.calls.clone();
        eredu_adapter::WorkerHandle::spawn(
            move || {
                let mut backend = MockBackend::new(
                    calls,
                    support::encode("<think>\nsecret\n</think>\n\nhello<|im_end|>"),
                );
                backend.delay = true;
                backend
            },
            resolved.model_path.clone(),
            model::ModelSettings {
                chat_template: ChatTemplate::CustomInline {
                    template: include_str!("qwen3.jinja").into(),
                },
                ..settings.clone()
            },
            None,
        )
        .map(|worker| Box::new(worker) as Box<dyn BackendLoadedModel>)
    }
    fn generate(
        &self,
        loaded: &mut dyn BackendLoadedModel,
        request: backend::LocalGenerationRequest<'_>,
    ) -> Result<(), ProviderError> {
        EreduBackend.generate(loaded, request)
    }
    fn available_memory_bytes(&self) -> u64 {
        0
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn provider_deduplicates_serializes_disconnects_evicts_and_finalizes_once() {
    let dir = tempfile::tempdir().unwrap();
    support::write_artifact(dir.path());
    let second = tempfile::tempdir().unwrap();
    support::write_artifact(second.path());
    let calls = Arc::new(StdMutex::new(Calls::default()));
    let factory: Arc<dyn LocalInferenceBackend> = Arc::new(MockFactory {
        calls: calls.clone(),
    });
    let runtime = Arc::new(InferenceRuntime {
        models: Default::default(),
        cold_load_lock: Mutex::new(()),
        backends: [(EREDU_BACKEND_ID, factory)].into(),
    });
    let provider = LocalInferenceProvider {
        runtime: runtime.clone(),
        name: "local".into(),
    };
    let model = ModelConfig::new(dir.path().to_str().unwrap());
    let messages = [Message::user().with_text("hello")];
    let first = provider
        .stream(&model, "test", &messages, &[])
        .await
        .unwrap();
    let next = provider
        .stream(&model, "test", &messages, &[])
        .await
        .unwrap();
    async fn consume(mut stream: MessageStream) {
        let mut finalizations = 0;
        let mut visible = String::new();
        while let Some(item) = stream.next().await {
            let (message, usage) = item.unwrap();
            if let Some(message) = message {
                for content in message.content {
                    if let MessageContent::Text(text) = content {
                        visible.push_str(&text.text);
                    }
                }
            }
            if let Some(usage) = usage {
                finalizations += 1;
                assert_eq!(
                    usage.usage.output_tokens,
                    Some(
                        support::encode("<think>\nsecret\n</think>\n\nhello<|im_end|>").len()
                            as i32
                    )
                );
                assert!(usage.stats.unwrap().time_to_first_token_ms.unwrap() < 100);
            }
        }
        assert_eq!(finalizations, 1);
        assert_eq!(visible, "\n\nhello");
    }
    tokio::join!(consume(first), consume(next));
    assert_eq!(calls.lock().unwrap().loads, 1);
    assert_eq!(calls.lock().unwrap().resets, 2);

    let disconnect = provider
        .stream(&model, "test", &messages, &[])
        .await
        .unwrap();
    while calls.lock().unwrap().configs.len() < 3 {
        tokio::task::yield_now().await;
    }
    drop(disconnect);
    consume(
        provider
            .stream(&model, "test", &messages, &[])
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(calls.lock().unwrap().loads, 1);
    assert_eq!(calls.lock().unwrap().resets, 4);

    let replacement = ModelConfig::new(second.path().to_str().unwrap());
    consume(
        provider
            .stream(&replacement, "test", &messages, &[])
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(calls.lock().unwrap().loads, 2);
    assert_eq!(calls.lock().unwrap().drops, 1);
    drop(provider);
    drop(runtime);
    assert_eq!(calls.lock().unwrap().drops, 2);
}

#[test]
fn realization_cache_identity_includes_every_loading_constraint() {
    let resolved = ResolvedModelPaths {
        model_path: "target".into(),
        draft_model_path: None,
        context_limit: 4096,
        settings: Default::default(),
        mmproj_path: None,
        backend_id: "eredu".into(),
        format: "safetensors".into(),
    };
    let key = |resolved: &ResolvedModelPaths| {
        ModelCacheKey::new(
            EREDU_BACKEND_ID,
            "model",
            resolved.settings.chat_template.clone(),
            resolved,
        )
    };
    let original = key(&resolved);
    assert_ne!(
        ModelCacheKey::new(
            "llamacpp",
            "model",
            resolved.settings.chat_template.clone(),
            &resolved
        ),
        original
    );
    for mutate in [
        |r: &mut ResolvedModelPaths| r.model_path = "other".into(),
        |r: &mut ResolvedModelPaths| r.draft_model_path = Some("draft".into()),
        |r: &mut ResolvedModelPaths| r.settings.device = Some("cpu:0".into()),
        |r: &mut ResolvedModelPaths| r.settings.max_cached_shards = Some(2),
        |r: &mut ResolvedModelPaths| {
            r.settings.chat_template = ChatTemplate::CustomInline {
                template: "new".into(),
            }
        },
    ] {
        let mut changed = resolved.clone();
        mutate(&mut changed);
        assert_ne!(key(&changed), original);
    }
    let mut changed = resolved;
    changed.settings.presence_penalty = Some(1.0);
    assert_eq!(key(&changed), original);
}
