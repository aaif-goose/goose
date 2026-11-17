use goose::config::declarative_providers::custom_providers_dir;
use goose::providers::canonical_cost::estimate_model_cost;
use goose_providers::conversation::token_usage::Usage;
use serde_json::json;

#[test]
fn custom_cache_prices_require_input_and_output_prices() {
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path().to_str().unwrap();
    let _guard = env_lock::lock_env([("GOOSE_PATH_ROOT", Some(root_path))]);
    let dir = custom_providers_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let usage = Usage::new(Some(1_000_000), Some(1_000_000), None)
        .with_cache_tokens(Some(200_000), Some(300_000));

    for (prices, expected) in [
        (json!({}), 12.25),
        (
            json!({"input_token_cost": 0.000001, "currency": "EUR",
                 "cache_read_token_cost": 0.0000002}),
            12.25,
        ),
        (json!({"output_token_cost": 0.000004}), 12.25),
        (
            json!({"input_token_cost": 0.000001, "output_token_cost": 0.000004}),
            5.05,
        ),
        (
            json!({"input_token_cost": 0.000001, "output_token_cost": 0.000004,
                 "cache_read_token_cost": 0.0000002}),
            4.84,
        ),
        (
            json!({"input_token_cost": 0.000001, "output_token_cost": 0.000004,
                 "cache_write_token_cost": 0.000003}),
            5.65,
        ),
        (
            json!({"input_token_cost": 0.000001, "output_token_cost": 0.000004,
                 "cache_read_token_cost": 0.0}),
            4.8,
        ),
        (
            json!({"input_token_cost": 0.000001, "output_token_cost": 0.000004,
                 "cache_write_token_cost": 0.0}),
            4.75,
        ),
        (
            json!({"input_token_cost": 0.0, "output_token_cost": 0.0}),
            0.0,
        ),
    ] {
        let mut model = prices;
        model["name"] = json!("gpt-4o");
        let config = json!({
            "name": "priced_gateway",
            "engine": "openai",
            "display_name": "Gateway",
            "api_key_env": "",
            "base_url": "https://example.invalid/v1",
            "models": [model],
            "requires_auth": false
        });
        std::fs::write(dir.join("priced_gateway.json"), config.to_string()).unwrap();

        let cost = estimate_model_cost("priced_gateway", "gpt-4o", &usage).unwrap();
        assert!(
            (cost - expected).abs() < 1e-9,
            "{config}: got {cost}, expected {expected}"
        );
    }
}

#[test]
fn custom_cache_prices_work_without_a_catalog_entry() {
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path().to_str().unwrap();
    let _guard = env_lock::lock_env([("GOOSE_PATH_ROOT", Some(root_path))]);
    let dir = custom_providers_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let mut config = json!({
        "name": "private_gateway",
        "engine": "openai",
        "display_name": "Gateway",
        "api_key_env": "",
        "base_url": "https://example.invalid/v1",
        "models": [{
            "name": "private-model",
            "input_token_cost": 0.00000242,
            "output_token_cost": 0.00001,
            "cache_read_token_cost": 0.000000242,
            "cache_write_token_cost": 0.000003025
        }],
        "requires_auth": false
    });
    let path = dir.join("private_gateway.json");
    std::fs::write(&path, config.to_string()).unwrap();

    let usage =
        Usage::new(Some(100_000), Some(0), None).with_cache_tokens(Some(50_000), Some(25_000));
    let cost = estimate_model_cost("private_gateway", "private-model", &usage).unwrap();
    assert!((cost - 0.148225).abs() < 1e-9, "got {cost}");

    config["models"][0]
        .as_object_mut()
        .unwrap()
        .remove("output_token_cost");
    std::fs::write(&path, config.to_string()).unwrap();
    let usage = Usage::new(Some(100_000), Some(100), None);
    assert!(estimate_model_cost("private_gateway", "private-model", &usage).is_none());
}
