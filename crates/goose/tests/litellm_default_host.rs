use goose::providers::litellm::LiteLLMProvider;

#[tokio::test]
async fn litellm_host_follows_the_configuration_contract() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let root = temp_dir.path().to_string_lossy().to_string();
    let _guard = env_lock::lock_env([
        ("GOOSE_PATH_ROOT", Some(root.as_str())),
        ("GOOSE_DISABLE_KEYRING", Some("1")),
        ("GOOSE_ADDITIONAL_CONFIG_FILES", None::<&str>),
        ("LITELLM_API_KEY", Some("test-key")),
        ("LITELLM_HOST", None::<&str>),
        ("LITELLM_CUSTOM_HEADERS", None::<&str>),
    ]);

    let provider = LiteLLMProvider::from_env(None)
        .await
        .expect("provider should use its default host");
    let debug = format!("{provider:?}");
    assert!(
        debug.contains("http://localhost:4000"),
        "missing host should stay on the advertised local proxy: {debug}"
    );
    assert!(
        !debug.contains("api.litellm.ai"),
        "missing host should not select a remote service: {debug}"
    );

    std::env::set_var("LITELLM_HOST", "https://proxy.example.test");
    let provider = LiteLLMProvider::from_env(None)
        .await
        .expect("provider should accept an explicit host");
    let debug = format!("{provider:?}");
    assert!(
        debug.contains("https://proxy.example.test"),
        "explicit host should remain unchanged: {debug}"
    );
}
