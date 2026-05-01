use super::*;
use crate::agents::extension_manager::get_tool_owner;
use crate::config::extensions::name_to_key;
use std::collections::HashSet;

const EXTENSION_MANAGER_KEY: &str = "extensionmanager";

fn global_enabled_for_config_save(config: &ExtensionConfig, requested_enabled: bool) -> bool {
    // Only Extension Manager starts globally; other extensions are catalog entries
    // that Extension Manager loads into a session on demand.
    requested_enabled && config.key() == EXTENSION_MANAGER_KEY
}

fn extension_config_to_dto(config: ExtensionConfig) -> ExtensionConfigDto {
    match config {
        ExtensionConfig::Sse {
            name,
            description,
            uri,
        } => ExtensionConfigDto::Sse {
            name,
            description,
            uri,
            bundled: None,
        },
        ExtensionConfig::Stdio {
            name,
            description,
            cmd,
            args,
            envs: _,
            env_keys,
            timeout,
            bundled,
            available_tools,
        } => ExtensionConfigDto::Stdio {
            name,
            description,
            cmd,
            args,
            envs: HashMap::new(),
            env_keys,
            timeout: timeout_to_dto(timeout),
            bundled,
            available_tools,
        },
        ExtensionConfig::Builtin {
            name,
            description,
            display_name,
            timeout,
            bundled,
            available_tools,
        } => ExtensionConfigDto::Builtin {
            name,
            description,
            display_name,
            timeout: timeout_to_dto(timeout),
            bundled,
            available_tools,
        },
        ExtensionConfig::Platform {
            name,
            description,
            display_name,
            bundled,
            available_tools,
        } => ExtensionConfigDto::Platform {
            name,
            description,
            display_name,
            bundled,
            available_tools,
        },
        ExtensionConfig::StreamableHttp {
            name,
            description,
            uri,
            envs: _,
            env_keys,
            headers: _,
            timeout,
            socket,
            bundled,
            available_tools,
        } => ExtensionConfigDto::StreamableHttp {
            name,
            description,
            uri,
            envs: HashMap::new(),
            env_keys,
            headers: HashMap::new(),
            timeout: timeout_to_dto(timeout),
            socket,
            bundled,
            available_tools,
        },
        ExtensionConfig::Frontend {
            name,
            description,
            tools,
            instructions,
            bundled,
            available_tools,
        } => ExtensionConfigDto::Frontend {
            name,
            description,
            frontend_tools: tools
                .into_iter()
                .filter_map(|tool| serde_json::to_value(tool).ok())
                .collect(),
            instructions,
            bundled,
            available_tools,
        },
        ExtensionConfig::InlinePython {
            name,
            description,
            code,
            timeout,
            dependencies,
            available_tools,
        } => ExtensionConfigDto::InlinePython {
            name,
            description,
            code,
            timeout: timeout_to_dto(timeout),
            dependencies,
            available_tools,
        },
    }
}

fn tool_extension_key(tool: &rmcp::model::Tool) -> Option<String> {
    get_tool_owner(tool)
        .map(|owner| name_to_key(&owner))
        .or_else(|| {
            tool.name
                .split_once("__")
                .map(|(owner, _)| name_to_key(owner))
        })
}

fn timeout_to_dto(timeout: Option<u64>) -> Option<u32> {
    timeout.and_then(|value| u32::try_from(value).ok())
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

    pub(super) async fn on_get_session_extension_status(
        &self,
        req: GetSessionExtensionStatusRequest,
    ) -> Result<GetSessionExtensionStatusResponse, sacp::Error> {
        let internal_id = self.internal_session_id(&req.session_id).await?;
        let session = self
            .session_manager
            .get_session(&internal_id, false)
            .await
            .internal_err()?;
        let expected_extensions = EnabledExtensionsState::extensions_or_default(
            Some(&session.extension_data),
            crate::config::Config::global(),
        );
        let agent = self.get_session_agent(&req.session_id, None).await?;
        let connected_extensions = agent.get_extension_configs().await;
        let connected_keys = connected_extensions
            .iter()
            .map(ExtensionConfig::key)
            .collect::<HashSet<_>>();
        let mut seen_keys = HashSet::new();
        let mut extensions = Vec::new();

        for extension in expected_extensions {
            seen_keys.insert(extension.key());
            extensions.push(extension);
        }

        for extension in connected_extensions {
            if seen_keys.insert(extension.key()) {
                extensions.push(extension);
            }
        }

        let mut tools_by_extension: HashMap<String, Vec<String>> = HashMap::new();
        for tool in agent.list_tools(&internal_id, None).await {
            if let Some(owner_key) = tool_extension_key(&tool) {
                tools_by_extension
                    .entry(owner_key)
                    .or_default()
                    .push(tool.name.to_string());
            }
        }

        let extensions_json = extensions
            .into_iter()
            .map(|extension| {
                let config_key = extension.key();
                let connected = connected_keys.contains(&config_key);
                let tools = if connected {
                    tools_by_extension.remove(&config_key).unwrap_or_default()
                } else {
                    Vec::new()
                };

                SessionExtensionStatusDto {
                    config: extension_config_to_dto(extension),
                    config_key,
                    status: if connected {
                        ExtensionConnectionStatusDto::Connected
                    } else {
                        ExtensionConnectionStatusDto::Failed
                    },
                    tools,
                    error: if connected {
                        None
                    } else {
                        Some(
                            "Goose could not connect this extension when the chat started."
                                .to_string(),
                        )
                    },
                }
            })
            .collect();

        Ok(GetSessionExtensionStatusResponse {
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
