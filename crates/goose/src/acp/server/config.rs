use super::*;

impl GooseAcpAgent {
    pub(super) async fn on_preferences_read(
        &self,
        req: PreferencesReadRequest,
    ) -> Result<PreferencesReadResponse, agent_client_protocol::Error> {
        let config = self.config()?;
        let keys = if req.keys.is_empty() {
            PREFERENCE_DEFS.iter().map(|def| def.key).collect()
        } else {
            req.keys
        };
        let mut values = Vec::with_capacity(keys.len());

        for key in keys {
            let def = preference_def(key)?;
            let value = match config.get_param::<serde_json::Value>(def.config_key) {
                Ok(value) => value,
                Err(crate::config::ConfigError::NotFound(_)) => serde_json::Value::Null,
                Err(e) => {
                    return Err(agent_client_protocol::Error::internal_error().data(e.to_string()))
                }
            };
            values.push(PreferenceValue { key, value });
        }

        Ok(PreferencesReadResponse { values })
    }

    pub(super) async fn on_preferences_save(
        &self,
        req: PreferencesSaveRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        let config = self.config()?;
        let mut updates = Vec::with_capacity(req.values.len());

        for preference in &req.values {
            let def = preference_def(preference.key)?;
            (def.validate)(&preference.value)?;
            updates.push((def.config_key.to_string(), preference.value.clone()));
        }

        config.set_param_values(&updates).internal_err()?;
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_preferences_remove(
        &self,
        req: PreferencesRemoveRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        let config = self.config()?;
        for key in req.keys {
            let def = preference_def(key)?;
            config.delete(def.config_key).internal_err()?;
        }
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_defaults_read(
        &self,
        _req: DefaultsReadRequest,
    ) -> Result<DefaultsReadResponse, agent_client_protocol::Error> {
        let config = self.config()?;
        Ok(DefaultsReadResponse {
            provider_id: optional_config_string(&config, "GOOSE_PROVIDER")?,
            model_id: optional_config_string(&config, "GOOSE_MODEL")?,
        })
    }

    pub(super) async fn on_defaults_save(
        &self,
        req: DefaultsSaveRequest,
    ) -> Result<DefaultsReadResponse, agent_client_protocol::Error> {
        let provider_id = req.provider_id.trim().to_string();
        if provider_id.is_empty() {
            return Err(
                agent_client_protocol::Error::invalid_params().data("providerId cannot be empty")
            );
        }

        let model_id = req.model_id.and_then(|model| {
            let model = model.trim().to_string();
            (!model.is_empty()).then_some(model)
        });

        let entries = self
            .provider_inventory
            .entries(std::slice::from_ref(&provider_id))
            .await
            .internal_err_ctx("Failed to read provider inventory")?;
        let Some(entry) = entries
            .into_iter()
            .find(|entry| entry.provider_id == provider_id)
        else {
            return Err(agent_client_protocol::Error::invalid_params()
                .data(format!("Unknown provider: {provider_id}")));
        };

        if !entry.configured {
            return Err(agent_client_protocol::Error::invalid_params()
                .data(format!("Provider is not configured: {provider_id}")));
        }

        if let Some(model_id) = model_id.as_deref() {
            let model_exists = entry.default_model == model_id
                || entry.models.iter().any(|model| model.id == model_id);
            if !model_exists {
                return Err(agent_client_protocol::Error::invalid_params().data(format!(
                    "Model '{model_id}' is not available for provider '{provider_id}'"
                )));
            }
        }

        let config = self.config()?;
        config
            .set_param_values(&[(
                "GOOSE_PROVIDER".to_string(),
                serde_json::Value::String(provider_id.clone()),
            )])
            .internal_err_ctx("Failed to save default provider")?;
        if let Some(model_id) = model_id.as_deref() {
            config
                .set_param("GOOSE_MODEL", model_id)
                .internal_err_ctx("Failed to save default model")?;
        } else {
            config
                .delete("GOOSE_MODEL")
                .internal_err_ctx("Failed to clear default model")?;
        }

        Ok(DefaultsReadResponse {
            provider_id: Some(provider_id),
            model_id,
        })
    }
}

struct PreferenceDef {
    key: PreferenceKey,
    config_key: &'static str,
    validate: fn(&serde_json::Value) -> Result<(), agent_client_protocol::Error>,
}

const PREFERENCE_DEFS: &[PreferenceDef] = &[
    PreferenceDef {
        key: PreferenceKey::AutoCompactThreshold,
        config_key: "GOOSE_AUTO_COMPACT_THRESHOLD",
        validate: validate_auto_compact_threshold,
    },
    PreferenceDef {
        key: PreferenceKey::VoiceAutoSubmitPhrases,
        config_key: "VOICE_AUTO_SUBMIT_PHRASES",
        validate: validate_voice_auto_submit_phrases,
    },
    PreferenceDef {
        key: PreferenceKey::VoiceDictationProvider,
        config_key: "VOICE_DICTATION_PROVIDER",
        validate: validate_voice_dictation_provider,
    },
    PreferenceDef {
        key: PreferenceKey::VoiceDictationPreferredMic,
        config_key: "VOICE_DICTATION_PREFERRED_MIC",
        validate: validate_voice_dictation_preferred_mic,
    },
];

fn preference_def(
    key: PreferenceKey,
) -> Result<&'static PreferenceDef, agent_client_protocol::Error> {
    PREFERENCE_DEFS
        .iter()
        .find(|def| def.key == key)
        .ok_or_else(|| {
            agent_client_protocol::Error::internal_error()
                .data(format!("Missing preference definition for {key:?}"))
        })
}

fn validate_auto_compact_threshold(
    value: &serde_json::Value,
) -> Result<(), agent_client_protocol::Error> {
    let Some(value) = value.as_f64() else {
        return Err(agent_client_protocol::Error::invalid_params()
            .data("autoCompactThreshold must be a number"));
    };
    if !value.is_finite() || value <= 0.0 || value > 1.0 {
        return Err(agent_client_protocol::Error::invalid_params()
            .data("autoCompactThreshold must be greater than 0 and at most 1"));
    }

    Ok(())
}

fn validate_voice_auto_submit_phrases(
    value: &serde_json::Value,
) -> Result<(), agent_client_protocol::Error> {
    if !value.is_string() {
        return Err(agent_client_protocol::Error::invalid_params()
            .data("voiceAutoSubmitPhrases must be a string"));
    }

    Ok(())
}

fn validate_voice_dictation_provider(
    value: &serde_json::Value,
) -> Result<(), agent_client_protocol::Error> {
    let Some(value) = value.as_str() else {
        return Err(agent_client_protocol::Error::invalid_params()
            .data("voiceDictationProvider must be a string"));
    };
    if !is_supported_voice_dictation_provider(value) {
        return Err(agent_client_protocol::Error::invalid_params()
            .data("voiceDictationProvider is not supported"));
    }

    Ok(())
}

fn validate_voice_dictation_preferred_mic(
    value: &serde_json::Value,
) -> Result<(), agent_client_protocol::Error> {
    let Some(value) = value.as_str() else {
        return Err(agent_client_protocol::Error::invalid_params()
            .data("voiceDictationPreferredMic must be a string"));
    };
    if value.is_empty() {
        return Err(agent_client_protocol::Error::invalid_params()
            .data("voiceDictationPreferredMic must be non-empty"));
    }

    Ok(())
}

fn is_supported_voice_dictation_provider(value: &str) -> bool {
    matches!(value, "openai" | "groq" | "elevenlabs" | "__disabled__") || {
        #[cfg(feature = "local-inference")]
        {
            value == "local"
        }
        #[cfg(not(feature = "local-inference"))]
        {
            false
        }
    }
}

fn optional_config_string(
    config: &Config,
    key: &str,
) -> Result<Option<String>, agent_client_protocol::Error> {
    match config.get_param::<String>(key) {
        Ok(value) => Ok(Some(value)),
        Err(crate::config::ConfigError::NotFound(_)) => Ok(None),
        Err(e) => Err(agent_client_protocol::Error::internal_error().data(e.to_string())),
    }
}
