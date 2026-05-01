use super::*;
use std::collections::HashSet;

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
            enabled: req.enabled,
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

        let mut extensions_json = Vec::new();
        for extension in extensions {
            let config_key = extension.key();
            let extension_name = extension.name();
            let connected = connected_keys.contains(&config_key);
            let tools = if connected {
                agent
                    .list_tools(&internal_id, Some(extension_name.clone()))
                    .await
                    .into_iter()
                    .map(|tool| tool.name.to_string())
                    .collect()
            } else {
                Vec::new()
            };
            extensions_json.push(SessionExtensionStatusDto {
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
                        "Goose could not connect this extension when the chat started.".to_string(),
                    )
                },
            });
        }

        Ok(GetSessionExtensionStatusResponse {
            extensions: extensions_json,
        })
    }
}
