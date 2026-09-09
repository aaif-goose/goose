#![cfg(feature = "hf-hub")]

use goose_local_inference::{
    management::{model_settings_from_dto, model_settings_to_dto},
    model::{ChatTemplate, ModelSettings, SamplingConfig, ToolCallingMode},
};

#[test]
fn settings_wire_round_trip_preserves_overrides_and_inheritance() {
    let explicit = ModelSettings {
        backend_id: Some("eredu".into()),
        device: Some("cpu:0".into()),
        max_cached_shards: Some(3),
        context_size: Some(4096),
        max_output_tokens: Some(128),
        draft_model: Some("draft-checkpoint".into()),
        sampling: SamplingConfig::Temperature {
            temperature: Some(0.7),
            top_k: Some(17),
            top_p: Some(0.85),
            min_p: Some(0.02),
            seed: Some(42),
        },
        repeat_penalty: Some(1.1),
        repeat_last_n: Some(32),
        frequency_penalty: Some(0.0),
        presence_penalty: Some(0.4),
        n_batch: Some(256),
        n_gpu_layers: Some(12),
        use_mlock: true,
        flash_attention: Some(false),
        n_threads: Some(6),
        tool_calling: ToolCallingMode::ForceEmulated,
        chat_template: ChatTemplate::CustomInline {
            template: "{{ messages[0].content }}".into(),
        },
        enable_thinking: Some(false),
        vision_capable: true,
        image_token_estimate: 512,
        mmproj_size_bytes: 8192,
    };
    let mut cases = vec![ModelSettings::default(), explicit.clone()];
    for sampling in [
        SamplingConfig::Greedy,
        SamplingConfig::Temperature {
            temperature: Some(0.0),
            top_k: None,
            top_p: None,
            min_p: None,
            seed: None,
        },
        SamplingConfig::MirostatV2 {
            temperature: None,
            tau: 4.0,
            eta: 0.2,
            seed: Some(23),
        },
    ] {
        cases.push(ModelSettings {
            sampling,
            tool_calling: ToolCallingMode::ForceNative,
            chat_template: ChatTemplate::Builtin {
                name: "chatml".into(),
            },
            ..explicit.clone()
        });
    }
    for settings in cases {
        let wire = serde_json::to_value(model_settings_to_dto(&settings)).unwrap();
        let restored = model_settings_from_dto(serde_json::from_value(wire).unwrap());
        assert_eq!(
            serde_json::to_value(restored).unwrap(),
            serde_json::to_value(settings).unwrap()
        );
    }
}
