//! The active catalog is process-global, so this file holds a single test to
//! keep its activation from racing other tests.

use goose_provider_types::canonical::{
    activate, maybe_get_canonical_model, CanonicalModelRegistry,
};
use goose_provider_types::context_limit::ContextLimitResolver;
use goose_provider_types::model::{ModelConfig, DEFAULT_CONTEXT_LIMIT};

#[test]
fn activated_catalog_replaces_model_lookups() {
    let bundled_hit = maybe_get_canonical_model("openai", "gpt-4o").is_some();
    assert_eq!(bundled_hit, cfg!(feature = "bundled-catalog"));

    let registry = CanonicalModelRegistry::from_json(
        r#"[{
            "id": "openai/embedder-model",
            "name": "Embedder Model",
            "tool_call": true,
            "modalities": { "input": ["text", "image"], "output": ["text"] },
            "limit": { "context": 32768, "output": 4096 }
        }]"#,
    )
    .unwrap();
    activate(registry).unwrap();

    let resolver = ContextLimitResolver::new("openai");
    assert_eq!(resolver.resolve_local("embedder-model", None), 32_768);
    assert_eq!(
        resolver.resolve_local("gpt-4o", None),
        DEFAULT_CONTEXT_LIMIT
    );

    let model = ModelConfig::new("embedder-model").with_canonical_limits("openai");
    assert_eq!(model.max_tokens, Some(4096));
    assert_eq!(model.supports_vision, Some(true));
}
