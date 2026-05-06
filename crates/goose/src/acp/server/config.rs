use super::*;

impl GooseAcpAgent {
    pub(super) async fn on_preferences_read(
        &self,
        req: PreferencesReadRequest,
    ) -> Result<PreferencesReadResponse, sacp::Error> {
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
                Err(e) => return Err(sacp::Error::internal_error().data(e.to_string())),
            };
            values.push(PreferenceValue { key, value });
        }

        Ok(PreferencesReadResponse { values })
    }

    pub(super) async fn on_preferences_save(
        &self,
        req: PreferencesSaveRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
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
    ) -> Result<EmptyResponse, sacp::Error> {
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
    ) -> Result<DefaultsReadResponse, sacp::Error> {
        let config = self.config()?;
        Ok(DefaultsReadResponse {
            provider_id: optional_config_string(&config, "GOOSE_PROVIDER")?,
            model_id: optional_config_string(&config, "GOOSE_MODEL")?,
        })
    }
}

struct PreferenceDef {
    key: PreferenceKey,
    config_key: &'static str,
    validate: fn(&serde_json::Value) -> Result<(), sacp::Error>,
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

fn preference_def(key: PreferenceKey) -> Result<&'static PreferenceDef, sacp::Error> {
    PREFERENCE_DEFS
        .iter()
        .find(|def| def.key == key)
        .ok_or_else(|| {
            sacp::Error::internal_error().data(format!("Missing preference definition for {key:?}"))
        })
}

fn validate_auto_compact_threshold(value: &serde_json::Value) -> Result<(), sacp::Error> {
    let Some(value) = value.as_f64() else {
        return Err(sacp::Error::invalid_params().data("autoCompactThreshold must be a number"));
    };
    if !value.is_finite() || value <= 0.0 || value > 1.0 {
        return Err(sacp::Error::invalid_params()
            .data("autoCompactThreshold must be greater than 0 and at most 1"));
    }

    Ok(())
}

fn validate_voice_auto_submit_phrases(value: &serde_json::Value) -> Result<(), sacp::Error> {
    if !value.is_string() {
        return Err(sacp::Error::invalid_params().data("voiceAutoSubmitPhrases must be a string"));
    }

    Ok(())
}

fn validate_voice_dictation_provider(value: &serde_json::Value) -> Result<(), sacp::Error> {
    let Some(value) = value.as_str() else {
        return Err(sacp::Error::invalid_params().data("voiceDictationProvider must be a string"));
    };
    if !is_supported_voice_dictation_provider(value) {
        return Err(sacp::Error::invalid_params().data("voiceDictationProvider is not supported"));
    }

    Ok(())
}

fn validate_voice_dictation_preferred_mic(value: &serde_json::Value) -> Result<(), sacp::Error> {
    let Some(value) = value.as_str() else {
        return Err(
            sacp::Error::invalid_params().data("voiceDictationPreferredMic must be a string")
        );
    };
    if value.is_empty() {
        return Err(
            sacp::Error::invalid_params().data("voiceDictationPreferredMic must be non-empty")
        );
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

fn optional_config_string(config: &Config, key: &str) -> Result<Option<String>, sacp::Error> {
    match config.get_param::<String>(key) {
        Ok(value) => Ok(Some(value)),
        Err(crate::config::ConfigError::NotFound(_)) => Ok(None),
        Err(e) => Err(sacp::Error::internal_error().data(e.to_string())),
    }
}
