use super::*;
const EXTENSION_MANAGER_KEY: &str = "extensionmanager";

fn global_enabled_for_config_save(config: &ExtensionConfig, requested_enabled: bool) -> bool {
    // Only Extension Manager starts globally; other extensions are catalog entries
    // that Extension Manager loads into a session on demand.
    requested_enabled && config.key() == EXTENSION_MANAGER_KEY
}

impl GooseAcpAgent {
    pub(super) async fn on_add_extension(
        &self,
        req: AddExtensionRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        let internal_id = self.internal_session_id(&req.session_id).await?;
        let config: ExtensionConfig = serde_json::from_value(req.config)
            .map_err(|e| sacp::Error::invalid_params().data(format!("bad config: {e}")))?;
        let agent = self.get_session_agent(&req.session_id, None).await?;
        agent
            .add_extension(config, &internal_id)
            .await
            .internal_err()?;
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_remove_extension(
        &self,
        req: RemoveExtensionRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        let internal_id = self.internal_session_id(&req.session_id).await?;
        let agent = self.get_session_agent(&req.session_id, None).await?;
        agent
            .remove_extension(&req.name, &internal_id)
            .await
            .internal_err()?;
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_get_extensions(&self) -> Result<GetExtensionsResponse, sacp::Error> {
        let extensions = crate::config::extensions::get_all_extensions();
        let warnings = crate::config::extensions::get_warnings();
        let extensions_json = extensions
            .into_iter()
            .map(|e| {
                let config_key = e.config.key();
                let mut value = serde_json::to_value(&e)?;
                if let Some(obj) = value.as_object_mut() {
                    obj.insert(
                        "config_key".to_string(),
                        serde_json::Value::String(config_key),
                    );
                }
                Ok::<_, serde_json::Error>(value)
            })
            .collect::<Result<Vec<_>, _>>()
            .internal_err()?;
        Ok(GetExtensionsResponse {
            extensions: extensions_json,
            warnings,
        })
    }

    pub(super) async fn on_add_config_extension(
        &self,
        req: AddConfigExtensionRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        let mut obj = match req.extension_config {
            serde_json::Value::Object(obj) => obj,
            _ => {
                return Err(
                    sacp::Error::invalid_params().data("extensionConfig must be a JSON object")
                );
            }
        };
        obj.insert(
            "name".to_string(),
            serde_json::Value::String(req.name.clone()),
        );

        let config: crate::agents::ExtensionConfig =
            serde_json::from_value(serde_json::Value::Object(obj))
                .map_err(|e| sacp::Error::invalid_params().data(format!("bad config: {e}")))?;

        crate::config::extensions::set_extension(crate::config::extensions::ExtensionEntry {
            enabled: global_enabled_for_config_save(&config, req.enabled),
            config,
        });
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_remove_config_extension(
        &self,
        req: RemoveConfigExtensionRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        let keys = crate::config::extensions::get_all_extension_names();
        if !keys.iter().any(|k| k == &req.config_key) {
            return Err(sacp::Error::invalid_params()
                .data(format!("Extension '{}' not found", req.config_key)));
        }
        crate::config::extensions::remove_extension(&req.config_key);
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_toggle_config_extension(
        &self,
        req: ToggleConfigExtensionRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        let keys = crate::config::extensions::get_all_extension_names();
        if !keys.iter().any(|k| k == &req.config_key) {
            return Err(sacp::Error::invalid_params()
                .data(format!("Extension '{}' not found", req.config_key)));
        }
        crate::config::extensions::set_extension_enabled(&req.config_key, req.enabled);
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_get_session_extensions(
        &self,
        req: GetSessionExtensionsRequest,
    ) -> Result<GetSessionExtensionsResponse, sacp::Error> {
        let internal_id = self.internal_session_id(&req.session_id).await?;
        let session = self
            .session_manager
            .get_session(&internal_id, false)
            .await
            .internal_err()?;

        let extensions = EnabledExtensionsState::extensions_or_default(
            Some(&session.extension_data),
            crate::config::Config::global(),
        );

        let extensions_json = extensions
            .into_iter()
            .map(|e| serde_json::to_value(&e))
            .collect::<Result<Vec<_>, _>>()
            .internal_err()?;

        Ok(GetSessionExtensionsResponse {
            extensions: extensions_json,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_save_disables_non_extension_manager_entries() {
        let config = ExtensionConfig::StreamableHttp {
            name: "Context7".to_string(),
            description: "Up-to-date code documentation".to_string(),
            uri: "https://mcp.context7.com/mcp".to_string(),
            envs: Default::default(),
            env_keys: Vec::new(),
            headers: Default::default(),
            timeout: Some(300),
            socket: None,
            bundled: Some(false),
            available_tools: Vec::new(),
        };

        assert!(!global_enabled_for_config_save(&config, true));
    }

    #[test]
    fn config_save_disables_legacy_enabled_custom_entries_on_edit() {
        let config = ExtensionConfig::Stdio {
            name: "github".to_string(),
            description: "Issue tracker".to_string(),
            cmd: "npx".to_string(),
            args: vec![
                "-y".to_string(),
                "@modelcontextprotocol/server-github".to_string(),
            ],
            envs: Default::default(),
            env_keys: Vec::new(),
            timeout: Some(300),
            bundled: Some(false),
            available_tools: Vec::new(),
        };

        assert!(!global_enabled_for_config_save(&config, true));
    }

    #[test]
    fn config_save_allows_extension_manager_to_be_enabled() {
        let config = ExtensionConfig::Platform {
            name: "Extension Manager".to_string(),
            description: "Enable extension management tools".to_string(),
            display_name: Some("Extension Manager".to_string()),
            bundled: Some(true),
            available_tools: Vec::new(),
        };

        assert!(global_enabled_for_config_save(&config, true));
        assert!(!global_enabled_for_config_save(&config, false));
    }
}
