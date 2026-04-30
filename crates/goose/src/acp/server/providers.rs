use super::*;

fn inventory_entry_to_dto(entry: ProviderInventoryEntry) -> ProviderInventoryEntryDto {
    let stale = ProviderInventoryService::is_stale(&entry);
    ProviderInventoryEntryDto {
        provider_id: entry.provider_id,
        provider_name: entry.provider_name,
        description: entry.description,
        default_model: entry.default_model,
        configured: entry.configured,
        provider_type: format!("{:?}", entry.provider_type),
        config_keys: entry
            .config_keys
            .into_iter()
            .map(provider_config_key_to_dto)
            .collect(),
        setup_steps: entry.setup_steps,
        supports_refresh: entry.supports_refresh,
        refreshing: entry.refreshing,
        models: entry
            .models
            .into_iter()
            .map(|m| ProviderInventoryModelDto {
                id: m.id,
                name: m.name,
                family: m.family,
                context_limit: m.context_limit,
                reasoning: m.reasoning,
                recommended: m.recommended,
            })
            .collect(),
        last_updated_at: entry.last_updated_at.map(|t| t.to_rfc3339()),
        last_refresh_attempt_at: entry.last_refresh_attempt_at.map(|t| t.to_rfc3339()),
        last_refresh_error: entry.last_refresh_error,
        stale,
        model_selection_hint: entry.model_selection_hint,
    }
}

fn provider_config_key_to_dto(key: crate::providers::base::ConfigKey) -> ProviderConfigKey {
    ProviderConfigKey {
        name: key.name,
        required: key.required,
        secret: key.secret,
        default: key.default,
        oauth_flow: key.oauth_flow,
        device_code_flow: key.device_code_flow,
        primary: key.primary,
    }
}

const SECRET_MASK_PREFIX_LEN: usize = 4;
const SECRET_MASK_SUFFIX_LEN: usize = 3;
const SECRET_MASK_FALLBACK: &str = "***";

fn mask_secret_value(value: &str) -> String {
    let prefix: String = value.chars().take(SECRET_MASK_PREFIX_LEN).collect();
    let suffix_chars: Vec<char> = value.chars().rev().take(SECRET_MASK_SUFFIX_LEN).collect();
    let suffix: String = suffix_chars.into_iter().rev().collect();

    if prefix.is_empty()
        || suffix.is_empty()
        || value.chars().count() <= SECRET_MASK_PREFIX_LEN + SECRET_MASK_SUFFIX_LEN
    {
        return SECRET_MASK_FALLBACK.to_string();
    }

    format!("{prefix}...{suffix}")
}

fn config_value_to_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Null => None,
        serde_json::Value::String(value) if value.is_empty() => None,
        serde_json::Value::String(value) => Some(value.clone()),
        other => serde_json::to_string(other).ok(),
    }
}

fn provider_config_field_value(
    config: &Config,
    key: &crate::providers::base::ConfigKey,
    secrets: Option<&HashMap<String, serde_json::Value>>,
) -> ProviderConfigFieldValueDto {
    let value = if key.secret {
        std::env::var(key.name.to_uppercase()).ok().or_else(|| {
            secrets
                .and_then(|values| values.get(&key.name))
                .and_then(config_value_to_string)
        })
    } else {
        config
            .get_param::<serde_json::Value>(&key.name)
            .ok()
            .and_then(|value| config_value_to_string(&value))
    };

    ProviderConfigFieldValueDto {
        key: key.name.clone(),
        value: value.as_deref().map(|value| {
            if key.secret {
                mask_secret_value(value)
            } else {
                value.to_string()
            }
        }),
        is_set: value.is_some(),
        is_secret: key.secret,
        required: key.required,
    }
}

fn refresh_skip_reason_to_dto(reason: RefreshSkipReason) -> RefreshProviderInventorySkipReasonDto {
    match reason {
        RefreshSkipReason::UnknownProvider => {
            RefreshProviderInventorySkipReasonDto::UnknownProvider
        }
        RefreshSkipReason::NotConfigured => RefreshProviderInventorySkipReasonDto::NotConfigured,
        RefreshSkipReason::DoesNotSupportRefresh => {
            RefreshProviderInventorySkipReasonDto::DoesNotSupportRefresh
        }
        RefreshSkipReason::AlreadyRefreshing => {
            RefreshProviderInventorySkipReasonDto::AlreadyRefreshing
        }
    }
}

fn refresh_plan_to_response(refresh_plan: RefreshPlan) -> RefreshProviderInventoryResponse {
    RefreshProviderInventoryResponse {
        started: refresh_plan.started,
        skipped: refresh_plan
            .skipped
            .into_iter()
            .map(|entry| RefreshProviderInventorySkipDto {
                provider_id: entry.provider_id,
                reason: refresh_skip_reason_to_dto(entry.reason),
            })
            .collect(),
    }
}

impl GooseAcpAgent {
    pub(super) async fn on_list_providers(
        &self,
        req: ListProvidersRequest,
    ) -> Result<ListProvidersResponse, sacp::Error> {
        let entries = self
            .provider_inventory
            .entries(&req.provider_ids)
            .await
            .internal_err()?;
        Ok(ListProvidersResponse {
            entries: entries.into_iter().map(inventory_entry_to_dto).collect(),
        })
    }

    pub(super) async fn provider_config_status(provider_id: String) -> ProviderConfigStatusDto {
        let is_configured = match crate::providers::get_from_registry(&provider_id).await {
            Ok(entry) => {
                match tokio::task::spawn_blocking(move || entry.inventory_configured()).await {
                    Ok(is_configured) => is_configured,
                    Err(error) => {
                        warn!(
                            provider = %provider_id,
                            error = %error,
                            "provider config status check failed"
                        );
                        false
                    }
                }
            }
            Err(_) => false,
        };

        ProviderConfigStatusDto {
            provider_id,
            is_configured,
        }
    }

    pub(super) async fn provider_config_statuses(
        provider_ids: &[String],
    ) -> Vec<ProviderConfigStatusDto> {
        let mut ids = if provider_ids.is_empty() {
            crate::providers::providers()
                .await
                .into_iter()
                .map(|(metadata, _)| metadata.name)
                .collect::<Vec<_>>()
        } else {
            provider_ids.to_vec()
        };
        ids.sort();
        ids.dedup();

        let mut statuses = stream::iter(ids)
            .map(Self::provider_config_status)
            .buffer_unordered(PROVIDER_CONFIG_STATUS_CHECK_CONCURRENCY)
            .collect::<Vec<_>>()
            .await;
        statuses.sort_by(|a, b| a.provider_id.cmp(&b.provider_id));
        statuses
    }

    pub(super) fn spawn_provider_inventory_refresh_jobs(&self, refresh_plan: &RefreshJobPlan) {
        for refresh_job in refresh_plan.started.iter().cloned() {
            let provider_inventory = self.provider_inventory.clone();
            let provider_factory = Arc::clone(&self.provider_factory);
            let provider_id = refresh_job.provider_id.clone();
            let identity = refresh_job.identity.clone();
            tokio::spawn(async move {
                let mut refresh_guard = provider_inventory.refresh_guard(&identity);
                let provider_result = AssertUnwindSafe(async {
                    let metadata = crate::providers::get_from_registry(&provider_id).await?;
                    let model_config =
                        crate::model::ModelConfig::new(&metadata.metadata().default_model)?
                            .with_canonical_limits(&provider_id);
                    provider_factory(provider_id.clone(), model_config, Vec::new()).await
                })
                .catch_unwind()
                .await;

                let fetch_result: Result<Vec<String>> = match provider_result {
                    Ok(Ok(provider)) => {
                        match ensure_refresh_identity_current(&provider_id, &identity).await {
                            Ok(()) => match AssertUnwindSafe(provider.fetch_recommended_models())
                                .catch_unwind()
                                .await
                            {
                                Ok(Ok(models)) => Ok(models),
                                Ok(Err(error)) => Err(anyhow::anyhow!(error.to_string())),
                                Err(_) => {
                                    Err(anyhow::anyhow!("provider inventory refresh task panicked"))
                                }
                            },
                            Err(error) => Err(error),
                        }
                    }
                    Ok(Err(error)) => Err(error),
                    Err(_) => Err(anyhow::anyhow!("provider inventory refresh task panicked")),
                };

                match fetch_result {
                    Ok(models) => match provider_inventory
                        .store_refreshed_models_for_identity(&identity, &models)
                        .await
                    {
                        Ok(()) => refresh_guard.complete(),
                        Err(error) => warn!(
                            provider = %provider_id,
                            error = %error,
                            "failed to store refreshed provider inventory"
                        ),
                    },
                    Err(error) => {
                        let error_message = error.to_string();
                        match provider_inventory
                            .store_refresh_error_for_identity(&identity, error_message.clone())
                            .await
                        {
                            Ok(()) => refresh_guard.complete(),
                            Err(store_error) => warn!(
                                provider = %provider_id,
                                error = %store_error,
                                refresh_error = %error_message,
                                "failed to store provider inventory refresh error"
                            ),
                        }
                        warn!(provider = %provider_id, error = %error_message, "provider inventory refresh failed");
                    }
                }
            });
        }
    }

    pub(super) async fn start_provider_inventory_refresh(
        &self,
        provider_ids: &[String],
    ) -> Result<RefreshProviderInventoryResponse, sacp::Error> {
        let refresh_job_plan = self
            .provider_inventory
            .plan_refresh_jobs(provider_ids)
            .await
            .internal_err()?;
        self.spawn_provider_inventory_refresh_jobs(&refresh_job_plan);
        Ok(refresh_plan_to_response(
            refresh_job_plan.into_public_plan(),
        ))
    }

    pub(super) async fn on_refresh_provider_inventory(
        &self,
        req: RefreshProviderInventoryRequest,
    ) -> Result<RefreshProviderInventoryResponse, sacp::Error> {
        Config::global().invalidate_secrets_cache();
        self.start_provider_inventory_refresh(&req.provider_ids)
            .await
    }

    pub(super) async fn on_read_provider_config(
        &self,
        req: ProviderConfigReadRequest,
    ) -> Result<ProviderConfigReadResponse, sacp::Error> {
        let entry = crate::providers::get_from_registry(&req.provider_id)
            .await
            .invalid_params_err_ctx("Unknown provider")?;
        let config = Config::global();
        let config_keys = &entry.metadata().config_keys;
        let secrets = if config_keys.iter().any(|key| key.secret) {
            Some(config.all_secrets().internal_err()?)
        } else {
            None
        };

        Ok(ProviderConfigReadResponse {
            fields: config_keys
                .iter()
                .map(|key| provider_config_field_value(config, key, secrets.as_ref()))
                .collect(),
        })
    }

    pub(super) async fn on_provider_config_status(
        &self,
        req: ProviderConfigStatusRequest,
    ) -> Result<ProviderConfigStatusResponse, sacp::Error> {
        Ok(ProviderConfigStatusResponse {
            statuses: Self::provider_config_statuses(&req.provider_ids).await,
        })
    }

    pub(super) async fn on_save_provider_config(
        &self,
        req: ProviderConfigSaveRequest,
    ) -> Result<ProviderConfigChangeResponse, sacp::Error> {
        let entry = crate::providers::get_from_registry(&req.provider_id)
            .await
            .invalid_params_err_ctx("Unknown provider")?;
        let metadata = entry.metadata().clone();
        let config = Config::global();
        let mut config_updates = Vec::new();
        let mut secret_updates = Vec::new();

        for field in &req.fields {
            let Some(config_key) = metadata
                .config_keys
                .iter()
                .find(|config_key| config_key.name == field.key)
            else {
                return Err(sacp::Error::invalid_params()
                    .data(format!("Unsupported provider config field: {}", field.key)));
            };

            let value = field.value.trim();
            if value.is_empty() {
                return Err(sacp::Error::invalid_params().data(format!(
                    "Provider config field cannot be empty: {}",
                    field.key
                )));
            }

            if config_key.secret {
                secret_updates.push((
                    config_key.name.clone(),
                    serde_json::Value::String(value.to_string()),
                ));
            } else {
                config_updates.push((config_key.name.clone(), value.to_string()));
            }
        }

        for (key, value) in config_updates {
            config
                .set_param(&key, &value)
                .internal_err_ctx("Failed to save provider config field")?;
        }
        config
            .set_secret_values(&secret_updates)
            .internal_err_ctx("Failed to save provider secret fields")?;

        let provider_ids = [req.provider_id.clone()];
        let status = Self::provider_config_status(req.provider_id.clone()).await;
        let refresh = self.start_provider_inventory_refresh(&provider_ids).await?;
        Ok(ProviderConfigChangeResponse { status, refresh })
    }

    pub(super) async fn on_delete_provider_config(
        &self,
        req: ProviderConfigDeleteRequest,
    ) -> Result<ProviderConfigChangeResponse, sacp::Error> {
        let entry = crate::providers::get_from_registry(&req.provider_id)
            .await
            .invalid_params_err_ctx("Unknown provider")?;
        let metadata = entry.metadata().clone();
        let config = Config::global();
        let mut secret_keys = Vec::new();

        for config_key in &metadata.config_keys {
            if config_key.secret {
                secret_keys.push(config_key.name.clone());
            } else {
                config
                    .delete(&config_key.name)
                    .internal_err_ctx("Failed to delete provider config field")?;
            }
        }

        config
            .delete_secret_values(&secret_keys)
            .internal_err_ctx("Failed to delete provider secret fields")?;
        crate::providers::cleanup_provider(&req.provider_id)
            .await
            .internal_err_ctx("Failed to clean up provider state")?;

        let provider_ids = [req.provider_id.clone()];
        let status = Self::provider_config_status(req.provider_id.clone()).await;
        let refresh = self.start_provider_inventory_refresh(&provider_ids).await?;
        Ok(ProviderConfigChangeResponse { status, refresh })
    }
}
