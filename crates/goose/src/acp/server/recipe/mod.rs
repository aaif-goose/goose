use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use agent_client_protocol::schema::v1::Meta;
use agent_client_protocol::{
    Client, ConnectionTo, JsonRpcMessage, JsonRpcRequest, JsonRpcResponse, UntypedMessage,
};
use fs_err as fs;
use goose_sdk_types::custom_requests::{
    DecodeRecipeRequest, DecodeRecipeResponse, DeleteRecipeRequest, EmptyResponse,
    EncodeRecipeRequest, EncodeRecipeResponse, ListRecipesRequest, ListRecipesResponse,
    ParseRecipeRequest, ParseRecipeResponse, RecipeDto, RecipeParameterDto, RecipeParamsAction,
    RecipeParamsResponse, RecipeToYamlRequest, RecipeToYamlResponse, RequestRecipeParams,
    SaveRecipeRequest, SaveRecipeResponse, ScanRecipeRequest, ScanRecipeResponse,
    ScheduleRecipeRequest, SetRecipeSlashCommandRequest, REQUEST_RECIPE_PARAMS_METHOD,
};
use tokio::sync::oneshot;

mod conversions;

use super::{meta_string, GooseAcpAgent, ResultExt};
use crate::agents::Agent;
use crate::recipe::build_recipe::{build_recipe_from_template, RecipeError};
use crate::recipe::local_recipes::{self, get_recipe_library_dir};
use crate::recipe::manifest::{
    list_recipe_file_manifests, load_recipe_from_path, short_id_from_path,
};
use crate::recipe::validate_recipe::validate_recipe_template_from_content;
use crate::recipe::{strip_error_location, Recipe, RecipeParameter};
use crate::recipe_deeplink;
use crate::session::{Session, SessionType};
use crate::slash_commands::recipe_slash_command;

use self::conversions::recipe_manifest_to_list_entry_dto;

pub(super) const RECIPE_PARAMS_METHOD: &str = REQUEST_RECIPE_PARAMS_METHOD;

pub(super) const RECIPE_PARAMS_CANCELLED_REASON: &str = "recipe_params_cancelled";

pub(super) fn deserialize_save_recipe_request(
    params: serde_json::Value,
) -> Result<SaveRecipeRequest, agent_client_protocol::Error> {
    let result: Result<SaveRecipeRequest, _> = serde_path_to_error::deserialize(params);
    result.map_err(save_recipe_validation_error)
}

impl GooseAcpAgent {
    pub(super) async fn resolve_recipe_from_meta(
        &self,
        meta: Option<&Meta>,
    ) -> Result<Option<(Recipe, PathBuf)>, agent_client_protocol::Error> {
        let resolved = if let Some(deeplink) = meta_string(meta, "recipeDeeplink")? {
            let recipe = recipe_deeplink::decode(&deeplink).map_err(|e| {
                agent_client_protocol::Error::invalid_params().data(format!("recipeDeeplink: {e}"))
            })?;
            Some((recipe, get_recipe_library_dir(true)))
        } else if let Some(id) = meta_string(meta, "recipeId")? {
            let path = self.resolve_recipe_path_by_id(&id).await?;
            let recipe = load_recipe_from_path(&path).internal_err_ctx("Failed to load recipe")?;
            let recipe_dir = path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| get_recipe_library_dir(true));
            Some((recipe, recipe_dir))
        } else {
            None
        };

        if let Some((ref recipe, ref recipe_dir)) = resolved {
            validate_recipe(recipe, recipe_dir)?;
        }
        Ok(resolved)
    }

    async fn resolve_recipe_path_by_id(
        &self,
        id: &str,
    ) -> Result<PathBuf, agent_client_protocol::Error> {
        if let Some(path) = self.recipe_path_cache.lock().await.get(id).cloned() {
            return Ok(path);
        }
        let map: HashMap<String, PathBuf> = list_recipe_file_manifests()
            .unwrap_or_default()
            .into_iter()
            .map(|manifest| (manifest.id, manifest.file_path))
            .collect();
        let resolved = map.get(id).cloned();
        *self.recipe_path_cache.lock().await = map;
        resolved.ok_or_else(|| {
            agent_client_protocol::Error::invalid_params().data(format!("recipe not found: {id}"))
        })
    }

    pub(super) async fn on_encode_recipe(
        &self,
        req: EncodeRecipeRequest,
    ) -> Result<EncodeRecipeResponse, agent_client_protocol::Error> {
        let recipe = recipe_from_dto(req.recipe)?;
        let deeplink = match recipe_deeplink::encode(&recipe) {
            Ok(deeplink) => deeplink,
            Err(err) => {
                tracing::error!("Failed to encode recipe: {}", err);
                #[cfg(feature = "telemetry")]
                crate::posthog::emit_error("recipe_encode_failed", &err.to_string());
                return Err(
                    agent_client_protocol::Error::invalid_params().data(format!("recipe: {err}"))
                );
            }
        };
        Ok(EncodeRecipeResponse { deeplink })
    }

    pub(super) async fn on_decode_recipe(
        &self,
        req: DecodeRecipeRequest,
    ) -> Result<DecodeRecipeResponse, agent_client_protocol::Error> {
        let recipe = match recipe_deeplink::decode(&req.deeplink) {
            Ok(recipe) => recipe,
            Err(err) => {
                tracing::error!("Failed to decode deeplink: {}", err);
                #[cfg(feature = "telemetry")]
                crate::posthog::emit_error("recipe_decode_failed", &err.to_string());
                return Err(
                    agent_client_protocol::Error::invalid_params().data(format!("deeplink: {err}"))
                );
            }
        };
        validate_recipe_without_dir(&recipe)?;
        Ok(DecodeRecipeResponse {
            recipe: recipe_to_dto(recipe)?,
        })
    }

    pub(super) async fn on_scan_recipe(
        &self,
        req: ScanRecipeRequest,
    ) -> Result<ScanRecipeResponse, agent_client_protocol::Error> {
        let recipe = recipe_from_dto(req.recipe)?;
        Ok(ScanRecipeResponse {
            has_security_warnings: recipe.check_for_security_warnings(),
        })
    }

    pub(super) async fn on_list_recipes(
        &self,
        _req: ListRecipesRequest,
    ) -> Result<ListRecipesResponse, agent_client_protocol::Error> {
        let manifests = list_recipe_file_manifests().internal_err_ctx("Failed to list recipes")?;
        let recipe_file_hash_map: HashMap<_, _> = manifests
            .iter()
            .map(|manifest| (manifest.id.clone(), manifest.file_path.clone()))
            .collect();
        *self.recipe_path_cache.lock().await = recipe_file_hash_map;

        let scheduled_jobs = match self.agent_manager.scheduler() {
            Some(scheduler) => scheduler.list_scheduled_jobs().await,
            None => Vec::new(),
        };
        let schedule_map: HashMap<_, _> = scheduled_jobs
            .into_iter()
            .map(|job| (PathBuf::from(job.source), job.cron))
            .collect();

        let slash_map: HashMap<_, _> = recipe_slash_command::list_commands()
            .into_iter()
            .map(|command| (PathBuf::from(command.recipe_path), command.command))
            .collect();

        let recipes = manifests
            .into_iter()
            .map(|manifest| {
                let schedule_cron = schedule_map.get(&manifest.file_path).cloned();
                let slash_command = slash_map.get(&manifest.file_path).cloned();
                recipe_manifest_to_list_entry_dto(
                    manifest.id,
                    manifest.recipe,
                    manifest.file_path.display(),
                    manifest.last_modified,
                    schedule_cron,
                    slash_command,
                )
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| agent_client_protocol::Error::internal_error().data(e.to_string()))?;

        Ok(ListRecipesResponse { recipes })
    }

    pub(super) async fn on_delete_recipe(
        &self,
        req: DeleteRecipeRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        let file_path = self.resolve_recipe_path_by_id(&req.id).await?;
        fs::remove_file(&file_path).internal_err_ctx("Failed to delete recipe")?;
        self.recipe_path_cache.lock().await.remove(&req.id);
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_schedule_recipe(
        &self,
        req: ScheduleRecipeRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        let scheduler = self.require_scheduler()?;
        let file_path = self.resolve_recipe_path_by_id(&req.id).await?;
        if let Err(err) = scheduler
            .schedule_recipe(file_path, req.cron_schedule)
            .await
        {
            tracing::error!("Failed to schedule recipe: {}", err);
            #[cfg(feature = "telemetry")]
            crate::posthog::emit_error("recipe_schedule_failed", &err.to_string());
            return Err(agent_client_protocol::Error::internal_error()
                .data(format!("Failed to schedule recipe: {err}")));
        }
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_set_recipe_slash_command(
        &self,
        req: SetRecipeSlashCommandRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        let file_path = self.resolve_recipe_path_by_id(&req.id).await?;
        if let Err(err) =
            recipe_slash_command::set_recipe_slash_command(file_path, req.slash_command)
        {
            tracing::error!("Failed to set slash command: {}", err);
            return Err(agent_client_protocol::Error::internal_error()
                .data(format!("Failed to set recipe slash command: {err}")));
        }
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_save_recipe(
        &self,
        req: SaveRecipeRequest,
    ) -> Result<SaveRecipeResponse, agent_client_protocol::Error> {
        let recipe = recipe_from_dto(req.recipe)?;
        if recipe.check_for_security_warnings() {
            return Err(agent_client_protocol::Error::invalid_params().data(
                "This recipe contains hidden characters that could be malicious. Please remove them before trying to save.",
            ));
        }
        validate_recipe_without_dir(&recipe)?;

        let file_path = match req.id.as_ref() {
            Some(id) => Some(self.resolve_recipe_path_by_id(id).await?),
            None => None,
        };

        let save_file_path = local_recipes::save_recipe_to_file(recipe, file_path)
            .internal_err_ctx("Failed to save recipe")?;
        let file_name = save_file_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        let file_path = save_file_path.display().to_string();
        let id = short_id_from_path(&file_path);
        self.recipe_path_cache
            .lock()
            .await
            .insert(id.clone(), save_file_path);

        Ok(SaveRecipeResponse {
            id,
            file_name,
            file_path,
        })
    }

    pub(super) async fn on_parse_recipe(
        &self,
        req: ParseRecipeRequest,
    ) -> Result<ParseRecipeResponse, agent_client_protocol::Error> {
        let recipe = validate_recipe_template_from_content(&req.content, None).map_err(|e| {
            agent_client_protocol::Error::invalid_params().data(format!("recipe: {e}"))
        })?;
        Ok(ParseRecipeResponse {
            recipe: recipe_to_dto(recipe)?,
        })
    }

    pub(super) async fn on_recipe_to_yaml(
        &self,
        req: RecipeToYamlRequest,
    ) -> Result<RecipeToYamlResponse, agent_client_protocol::Error> {
        let recipe = recipe_from_dto(req.recipe)?;
        let yaml = recipe.to_yaml().invalid_params_err_ctx("recipe")?;
        Ok(RecipeToYamlResponse { yaml })
    }

    fn render_recipe(
        &self,
        recipe: &Recipe,
        recipe_dir: &Path,
        values: HashMap<String, String>,
    ) -> Result<Option<Recipe>, agent_client_protocol::Error> {
        let content = recipe.to_yaml().map_err(|e| {
            agent_client_protocol::Error::invalid_params().data(format!("recipe: {e}"))
        })?;
        let params: Vec<(String, String)> = values.into_iter().collect();
        match build_recipe_from_template(
            content,
            recipe_dir,
            params,
            None::<fn(&str, &str) -> Result<String, anyhow::Error>>,
        ) {
            Ok(rendered) => Ok(Some(rendered)),
            Err(RecipeError::MissingParams { .. }) => Ok(None),
            Err(e) => {
                Err(agent_client_protocol::Error::internal_error().data(format!("recipe: {e}")))
            }
        }
    }

    pub(super) async fn apply_recipe(
        &self,
        agent: &Arc<Agent>,
        recipe: &Recipe,
    ) -> Result<(), agent_client_protocol::Error> {
        agent
            .apply_recipe_components(recipe.response.clone(), true)
            .await
            .invalid_params_err()?;
        if let Some(instructions) = recipe.instructions.clone() {
            agent
                .extend_system_prompt("recipe".to_string(), instructions)
                .await;
        }
        Ok(())
    }

    pub(super) async fn apply_session_recipe(
        &self,
        agent: &Arc<Agent>,
        session: &Session,
    ) -> Result<(), agent_client_protocol::Error> {
        let Some(recipe) = session.recipe.as_ref() else {
            return Ok(());
        };

        if session.session_type == SessionType::Scheduled {
            self.apply_recipe(agent, recipe).await?;
            return Ok(());
        }

        let recipe_dir = get_recipe_library_dir(true);
        if let Some(rendered) = self.render_recipe(
            recipe,
            &recipe_dir,
            session.user_recipe_values.clone().unwrap_or_default(),
        )? {
            self.apply_recipe(agent, &rendered).await?;
        }

        Ok(())
    }

    pub(super) async fn render_recipe_for_session(
        &self,
        cx: &ConnectionTo<Client>,
        session_id: &str,
        recipe: Option<&(Recipe, PathBuf)>,
        parameter_scope_id: Option<&str>,
    ) -> Result<(Option<Recipe>, Option<HashMap<String, String>>), agent_client_protocol::Error>
    {
        let Some((recipe, recipe_dir)) = recipe else {
            return Ok((None, None));
        };
        let (rendered, values) = self
            .render_recipe_with_params(cx, session_id, recipe, recipe_dir, parameter_scope_id)
            .await?;
        Ok((Some(rendered), values))
    }

    async fn render_recipe_with_params(
        &self,
        cx: &ConnectionTo<Client>,
        session_id: &str,
        recipe: &Recipe,
        recipe_dir: &Path,
        parameter_scope_id: Option<&str>,
    ) -> Result<(Recipe, Option<HashMap<String, String>>), agent_client_protocol::Error> {
        let parameters = recipe.parameters.clone().unwrap_or_default();

        if parameters.is_empty() || !self.supports_recipe_param_requests() {
            return match self.render_recipe(recipe, recipe_dir, HashMap::new())? {
                Some(rendered) => Ok((rendered, None)),
                None => Err(agent_client_protocol::Error::invalid_params().data(
                    "recipe requires parameters but the client does not support recipeParameterRequests",
                )),
            };
        }

        let response = self
            .request_recipe_params(cx, session_id, parameters, parameter_scope_id)
            .await?;
        if matches!(response.action, RecipeParamsAction::Cancel) {
            return Err(recipe_params_cancelled_error());
        }
        let values = response.values;
        match self.render_recipe(recipe, recipe_dir, values.clone())? {
            Some(rendered) => Ok((rendered, Some(values))),
            None => Err(agent_client_protocol::Error::invalid_params()
                .data("recipe still missing required parameters")),
        }
    }

    async fn request_recipe_params(
        &self,
        cx: &ConnectionTo<Client>,
        session_id: &str,
        parameters: Vec<RecipeParameter>,
        parameter_scope_id: Option<&str>,
    ) -> Result<RecipeParamsResponse, agent_client_protocol::Error> {
        let request = RequestRecipeParams {
            session_id: session_id.to_string(),
            parameters: parameters
                .into_iter()
                .map(RecipeParameterDto::from)
                .collect(),
            parameter_scope_id: parameter_scope_id.map(str::to_string),
        };
        let (tx, rx) = oneshot::channel();
        cx.send_request(RequestRecipeParamsMessage(request))
            .on_receiving_result(move |result| async move {
                let _ = tx.send(result.map(|response| response.0));
                Ok(())
            })?;
        match rx.await {
            Ok(response) => response,
            Err(_) => Err(agent_client_protocol::Error::internal_error()
                .data("recipe params request was dropped")),
        }
    }
}

fn recipe_params_cancelled_error() -> agent_client_protocol::Error {
    agent_client_protocol::Error::invalid_params().data(serde_json::json!({
        "reason": RECIPE_PARAMS_CANCELLED_REASON,
    }))
}

fn save_recipe_validation_error(
    error: serde_path_to_error::Error<serde_json::Error>,
) -> agent_client_protocol::Error {
    let path = error.path().to_string();
    let inner = strip_error_location(&error.into_inner().to_string());
    let message = if path == "." {
        format!("Save recipe validation failed: {inner}")
    } else {
        format!(
            "save recipe validation failed at {}: {inner}",
            path.trim_start_matches('.')
        )
    };
    agent_client_protocol::Error::invalid_params().data(message)
}

fn validate_recipe(recipe: &Recipe, recipe_dir: &Path) -> Result<(), agent_client_protocol::Error> {
    let yaml = recipe
        .to_yaml()
        .map_err(|e| agent_client_protocol::Error::invalid_params().data(format!("recipe: {e}")))?;
    validate_recipe_template_from_content(&yaml, Some(recipe_dir.to_string_lossy().to_string()))
        .map_err(|e| agent_client_protocol::Error::invalid_params().data(format!("recipe: {e}")))?;
    Ok(())
}

fn validate_recipe_without_dir(recipe: &Recipe) -> Result<(), agent_client_protocol::Error> {
    let yaml = recipe
        .to_yaml()
        .map_err(|e| agent_client_protocol::Error::invalid_params().data(format!("recipe: {e}")))?;
    validate_recipe_template_from_content(&yaml, None)
        .map_err(|e| agent_client_protocol::Error::invalid_params().data(format!("recipe: {e}")))?;
    Ok(())
}

fn recipe_from_dto(dto: RecipeDto) -> Result<Recipe, agent_client_protocol::Error> {
    Recipe::try_from(dto)
        .map_err(|e| agent_client_protocol::Error::invalid_params().data(format!("recipe: {e}")))
}

fn recipe_to_dto(recipe: Recipe) -> Result<RecipeDto, agent_client_protocol::Error> {
    RecipeDto::try_from(recipe)
        .map_err(|e| agent_client_protocol::Error::invalid_params().data(format!("recipe: {e}")))
}

#[derive(Debug, Clone)]
struct RequestRecipeParamsMessage(RequestRecipeParams);

impl JsonRpcMessage for RequestRecipeParamsMessage {
    fn matches_method(method: &str) -> bool {
        method == RECIPE_PARAMS_METHOD
    }

    fn method(&self) -> &str {
        RECIPE_PARAMS_METHOD
    }

    fn to_untyped_message(&self) -> Result<UntypedMessage, agent_client_protocol::Error> {
        UntypedMessage::new(RECIPE_PARAMS_METHOD, &self.0)
    }

    fn parse_message(
        method: &str,
        params: &impl serde::Serialize,
    ) -> Result<Self, agent_client_protocol::Error> {
        if !Self::matches_method(method) {
            return Err(agent_client_protocol::Error::method_not_found());
        }
        Ok(Self(agent_client_protocol::util::json_cast_params(params)?))
    }
}

impl JsonRpcRequest for RequestRecipeParamsMessage {
    type Response = RecipeParamsResponseMessage;
}

#[derive(Debug, Clone)]
struct RecipeParamsResponseMessage(RecipeParamsResponse);

impl JsonRpcResponse for RecipeParamsResponseMessage {
    fn into_json(self, _method: &str) -> Result<serde_json::Value, agent_client_protocol::Error> {
        serde_json::to_value(self.0).map_err(agent_client_protocol::Error::into_internal_error)
    }

    fn from_value(
        _method: &str,
        value: serde_json::Value,
    ) -> Result<Self, agent_client_protocol::Error> {
        Ok(Self(agent_client_protocol::util::json_cast(&value)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn error_data(error: agent_client_protocol::Error) -> String {
        error.data.unwrap().as_str().unwrap().to_string()
    }

    #[test]
    fn deserialize_save_recipe_request_reports_nested_path() {
        let error = deserialize_save_recipe_request(json!({
            "recipe": {
                "title": "Test",
                "description": "Test recipe",
                "prompt": "Run the test",
                "parameters": [
                    {
                        "key": "name",
                        "input_type": "bogus",
                        "requirement": "required",
                        "description": "Name"
                    }
                ]
            }
        }))
        .unwrap_err();

        let message = error_data(error);
        assert!(
            message.starts_with(
                "save recipe validation failed at recipe.parameters[0].input_type: unknown variant `bogus`"
            ),
            "{message}"
        );
    }

    #[test]
    fn deserialize_save_recipe_request_omits_root_path() {
        let error = deserialize_save_recipe_request(json!("not an object")).unwrap_err();

        let message = error_data(error);
        assert!(
            message.starts_with("Save recipe validation failed: invalid type: string"),
            "{message}"
        );
    }
}
