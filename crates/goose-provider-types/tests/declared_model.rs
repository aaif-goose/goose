use goose_provider_types::base::{find_declared_model, ModelInfo};

#[test]
fn exact_model_ids_win_over_case_insensitive_collisions() {
    let models = [
        ModelInfo::new("Foo").with_vision_support(false),
        ModelInfo::new("foo").with_vision_support(true),
    ];

    assert_eq!(
        find_declared_model(&models, "foo").unwrap().supports_vision,
        Some(true)
    );
    assert_eq!(
        find_declared_model(&models, "Foo").unwrap().supports_vision,
        Some(false)
    );
    assert!(find_declared_model(&models, "FOO").is_none());
}

#[test]
fn case_insensitive_fallback_requires_a_unique_model() {
    let models = [ModelInfo::new("Foo").with_vision_support(true)];

    assert_eq!(find_declared_model(&models, "FOO").unwrap().name, "Foo");
    assert!(find_declared_model(&models, "another-model").is_none());
    assert!(find_declared_model(&[], "Foo").is_none());
}
