use super::*;

impl GooseAcpAgent {
    pub(super) async fn on_preferences_read(
        &self,
        req: PreferencesReadRequest,
    ) -> Result<PreferencesReadResponse, sacp::Error> {
        let config = self.config()?;
        let keys = if req.keys.is_empty() {
            supported_preference_keys()
        } else {
            req.keys
        };
        let mut values = Vec::with_capacity(keys.len());

        for key in keys {
            let config_key = preference_config_key(&key);
            let value = match config.get_param::<serde_json::Value>(config_key) {
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
        for preference in req.values {
            validate_preference_value(&preference)?;
            config
                .set_param(preference_config_key(&preference.key), &preference.value)
                .internal_err()?;
        }
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_preferences_remove(
        &self,
        req: PreferencesRemoveRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        let config = self.config()?;
        for key in req.keys {
            config.delete(preference_config_key(&key)).internal_err()?;
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

fn supported_preference_keys() -> Vec<PreferenceKey> {
    vec![
        PreferenceKey::AutoCompactThreshold,
        PreferenceKey::VoiceAutoSubmitPhrases,
        PreferenceKey::VoiceDictationProvider,
        PreferenceKey::VoiceDictationPreferredMic,
    ]
}

fn preference_config_key(key: &PreferenceKey) -> &'static str {
    match key {
        PreferenceKey::AutoCompactThreshold => "GOOSE_AUTO_COMPACT_THRESHOLD",
        PreferenceKey::VoiceAutoSubmitPhrases => "VOICE_AUTO_SUBMIT_PHRASES",
        PreferenceKey::VoiceDictationProvider => "VOICE_DICTATION_PROVIDER",
        PreferenceKey::VoiceDictationPreferredMic => "VOICE_DICTATION_PREFERRED_MIC",
    }
}

fn validate_preference_value(preference: &PreferenceValue) -> Result<(), sacp::Error> {
    match preference.key {
        PreferenceKey::AutoCompactThreshold => {
            let Some(value) = preference.value.as_f64() else {
                return Err(
                    sacp::Error::invalid_params().data("autoCompactThreshold must be a number")
                );
            };
            if value <= 0.0 || value > 1.0 {
                return Err(sacp::Error::invalid_params()
                    .data("autoCompactThreshold must be greater than 0 and at most 1"));
            }
        }
        PreferenceKey::VoiceAutoSubmitPhrases => {
            if !preference.value.is_string() {
                return Err(
                    sacp::Error::invalid_params().data("voiceAutoSubmitPhrases must be a string")
                );
            }
        }
        PreferenceKey::VoiceDictationProvider => {
            let Some(value) = preference.value.as_str() else {
                return Err(
                    sacp::Error::invalid_params().data("voiceDictationProvider must be a string")
                );
            };
            if !matches!(
                value,
                "openai" | "groq" | "elevenlabs" | "local" | "__disabled__"
            ) {
                return Err(
                    sacp::Error::invalid_params().data("voiceDictationProvider is not supported")
                );
            }
        }
        PreferenceKey::VoiceDictationPreferredMic => {
            let Some(value) = preference.value.as_str() else {
                return Err(sacp::Error::invalid_params()
                    .data("voiceDictationPreferredMic must be a string"));
            };
            if value.is_empty() {
                return Err(sacp::Error::invalid_params()
                    .data("voiceDictationPreferredMic must be non-empty"));
            }
        }
    }

    Ok(())
}

fn optional_config_string(config: &Config, key: &str) -> Result<Option<String>, sacp::Error> {
    match config.get_param::<String>(key) {
        Ok(value) => Ok(Some(value)),
        Err(crate::config::ConfigError::NotFound(_)) => Ok(None),
        Err(e) => Err(sacp::Error::internal_error().data(e.to_string())),
    }
}
