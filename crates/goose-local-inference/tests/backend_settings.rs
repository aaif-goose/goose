#![cfg(feature = "hf-hub")]

use goose_local_inference::{config_resolver, management, model::ModelSettings};
use std::sync::Mutex;

static SETTINGS: Mutex<Option<ModelSettings>> = Mutex::new(None);
static GLOBAL_BACKEND: Mutex<Option<String>> = Mutex::new(None);

#[tokio::test]
async fn model_settings_separate_format_effective_backend_and_saved_override() {
    config_resolver::set_model_settings_resolver(|_| Ok(SETTINGS.lock().unwrap().clone()));
    config_resolver::set_model_settings_writer(|_, settings| {
        *SETTINGS.lock().unwrap() = Some(settings.clone());
        Ok(())
    });
    config_resolver::set_string_param_resolver(|key| {
        Ok((key == "GOOSE_LOCAL_BACKEND")
            .then(|| GLOBAL_BACKEND.lock().unwrap().clone())
            .flatten())
    });
    let dir = tempfile::tempdir().unwrap();
    let gguf = dir.path().join("model.gguf");
    std::fs::write(&gguf, []).unwrap();
    let id = gguf.to_str().unwrap();
    let info = management::get_model_settings(id).await.unwrap();
    assert_eq!(info.format.as_deref(), Some("gguf"));
    assert_eq!(info.backend_id.as_deref(), Some("llamacpp"));
    assert_eq!(info.settings.backend_id, None);

    let mut overrides = info.settings;
    overrides.backend_id = Some("eredu".into());
    management::update_model_settings(id, overrides.clone()).unwrap();
    let info = management::get_model_settings(id).await.unwrap();
    assert_eq!(info.format.as_deref(), Some("gguf"));
    assert_eq!(info.backend_id.as_deref(), Some("eredu"));
    assert_eq!(info.default_backend_id.as_deref(), Some("llamacpp"));
    overrides.backend_id = None;
    management::update_model_settings(id, overrides.clone()).unwrap();
    assert_eq!(
        management::get_model_settings(id)
            .await
            .unwrap()
            .backend_id
            .as_deref(),
        Some("llamacpp")
    );

    *GLOBAL_BACKEND.lock().unwrap() = Some("eredu".into());
    let info = management::get_model_settings(id).await.unwrap();
    assert_eq!(info.backend_id.as_deref(), Some("eredu"));
    assert_eq!(info.settings.backend_id, None);
    assert_eq!(info.default_backend_id.as_deref(), Some("eredu"));

    std::fs::write(dir.path().join("config.json"), "{}").unwrap();
    std::fs::write(dir.path().join("tokenizer.json"), "{}").unwrap();
    std::fs::write(dir.path().join("model.safetensors"), []).unwrap();
    let id = dir.path().to_str().unwrap();
    *GLOBAL_BACKEND.lock().unwrap() = None;
    let info = management::get_model_settings(id).await.unwrap();
    assert_eq!(info.format.as_deref(), Some("safetensors"));
    assert_eq!(info.backend_id.as_deref(), Some("eredu"));
    assert!(!info.available_backends.iter().any(|id| id == "llamacpp"));

    overrides.backend_id = Some("llamacpp".into());
    management::update_model_settings(id, overrides.clone()).unwrap();
    let info = management::get_model_settings(id).await.unwrap();
    assert_eq!(info.format.as_deref(), Some("safetensors"));
    assert!(info.effective_generation.unwrap()["error"]
        .as_str()
        .unwrap()
        .contains("does not support"));
    overrides.backend_id = None;
    management::update_model_settings(id, overrides).unwrap();
    assert_eq!(
        management::get_model_settings(id)
            .await
            .unwrap()
            .backend_id
            .as_deref(),
        Some("eredu")
    );
}
