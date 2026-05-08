use goose::agents::extension::ExtensionConfig;
use goose::agents::extension_manager::resolve_timeout;
use goose::config::DEFAULT_EXTENSION_TIMEOUT;
use serde_json;
use std::collections::HashMap;

#[test]
fn resolve_timeout_uses_per_extension_value() {
    let timeout = Some(120);
    let resolved = resolve_timeout(timeout);
    assert_eq!(resolved, 120);
}

#[test]
fn resolve_timeout_uses_default_when_none() {
    let timeout = None;
    let resolved = resolve_timeout(timeout);
    assert_eq!(resolved, DEFAULT_EXTENSION_TIMEOUT);
}

#[test]
fn streamable_http_config_timeout_roundtrips() {
    let config = ExtensionConfig::StreamableHttp {
        name: "test-extension".to_string(),
        description: "Test extension".to_string(),
        uri: "http://localhost:8080".to_string(),
        envs: Default::default(),
        env_keys: Vec::new(),
        headers: HashMap::new(),
        timeout: Some(42),
        socket: None,
        bundled: None,
        available_tools: Vec::new(),
    };

    let json = serde_json::to_string(&config).expect("Failed to serialize");
    let deserialized: ExtensionConfig = serde_json::from_str(&json).expect("Failed to deserialize");

    match deserialized {
        ExtensionConfig::StreamableHttp { timeout, .. } => {
            assert_eq!(timeout, Some(42));
        }
        _ => panic!("Expected StreamableHttp variant"),
    }
}
