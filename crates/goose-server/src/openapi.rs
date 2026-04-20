use goose::agents::extension::Envs;
use goose::agents::extension::ToolInfo;
use goose::agents::ExtensionConfig;
use goose::config::permission::PermissionLevel;
use goose::config::ExtensionEntry;
use goose::conversation::Conversation;
use goose::download_manager::{DownloadProgress, DownloadStatus};
use goose::model::ModelConfig;
use goose::permission::permission_confirmation::{Permission, PrincipalType};
use goose::providers::base::{ConfigKey, ModelInfo, ProviderMetadata, ProviderType};
use goose::session::{Session, SessionInsights, SessionType, SystemInfo};
use rmcp::model::{
    Annotations, Content, EmbeddedResource, Icon, IconTheme, ImageContent, JsonObject,
    RawAudioContent, RawContent, RawEmbeddedResource, RawImageContent, RawResource, RawTextContent,
    ResourceContents, Role, TaskSupport, TextContent, Tool, ToolAnnotations, ToolExecution,
};
use utoipa::{OpenApi, ToSchema};

use goose::config::declarative_providers::{
    DeclarativeProviderConfig, EnvVarConfig, LoadedProvider, ProviderEngine,
};
use goose::conversation::message::{
    ActionRequired, ActionRequiredData, FrontendToolRequest, Message, MessageContent,
    MessageMetadata, RedactedThinkingContent, SystemNotificationContent, SystemNotificationType,
    ThinkingContent, TokenState, ToolConfirmationRequest, ToolRequest, ToolResponse,
};

use crate::routes::recipe_utils::RecipeManifest;
use crate::routes::reply::MessageEvent;
use utoipa::openapi::schema::{
    AdditionalProperties, AnyOfBuilder, ArrayBuilder, ObjectBuilder, OneOfBuilder, Schema,
    SchemaFormat, SchemaType,
};
use utoipa::openapi::{AllOfBuilder, Ref, RefOr};

macro_rules! derive_utoipa {
    ($inner_type:ident as $schema_name:ident) => {
        pub struct $schema_name {}

        impl<'__s> ToSchema<'__s> for $schema_name {
            fn schema() -> (&'__s str, utoipa::openapi::RefOr<utoipa::openapi::Schema>) {
                let settings = rmcp::schemars::generate::SchemaSettings::openapi3();
                let generator = settings.into_generator();
                let schema = generator.into_root_schema_for::<$inner_type>();
                let schema = convert_schemars_to_utoipa(schema);
                (stringify!($inner_type), schema)
            }

            fn aliases() -> Vec<(&'__s str, utoipa::openapi::schema::Schema)> {
                Vec::new()
            }
        }
    };
    ($inner_type:ident as $schema_name:ident => $output_name:expr) => {
        pub struct $schema_name {}

        impl<'__s> ToSchema<'__s> for $schema_name {
            fn schema() -> (&'__s str, utoipa::openapi::RefOr<utoipa::openapi::Schema>) {
                let settings = rmcp::schemars::generate::SchemaSettings::openapi3();
                let generator = settings.into_generator();
                let schema = generator.into_root_schema_for::<$inner_type>();
                let schema = convert_schemars_to_utoipa(schema);
                ($output_name, schema)
            }

            fn aliases() -> Vec<(&'__s str, utoipa::openapi::schema::Schema)> {
                Vec::new()
            }
        }
    };
}

fn convert_schemars_to_utoipa(schema: rmcp::schemars::Schema) -> RefOr<Schema> {
    if let Some(true) = schema.as_bool() {
        return RefOr::T(Schema::Object(ObjectBuilder::new().build()));
    }

    if let Some(false) = schema.as_bool() {
        return RefOr::T(Schema::Object(ObjectBuilder::new().build()));
    }

    if let Some(obj) = schema.as_object() {
        return convert_json_object_to_utoipa(obj);
    }

    RefOr::T(Schema::Object(ObjectBuilder::new().build()))
}

fn convert_json_object_to_utoipa(
    obj: &serde_json::Map<String, serde_json::Value>,
) -> RefOr<Schema> {
    use serde_json::Value;

    if let Some(Value::String(reference)) = obj.get("$ref") {
        return RefOr::Ref(Ref::new(reference.clone()));
    }

    if let Some(Value::Array(one_of)) = obj.get("oneOf") {
        let mut builder = OneOfBuilder::new();
        for item in one_of {
            if let Ok(schema) = rmcp::schemars::Schema::try_from(item.clone()) {
                builder = builder.item(convert_schemars_to_utoipa(schema));
            }
        }
        return RefOr::T(Schema::OneOf(builder.build()));
    }

    // Handle the discriminated union pattern from schemars: an object with
    // `type`, `properties`, `required` AND `allOf` (e.g. each variant of a
    // `#[serde(tag = "type")]` enum). We merge the inline object (which carries
    // the discriminator property) with the `allOf` refs into a single `allOf`.
    if let Some(Value::Array(all_of)) = obj.get("allOf") {
        let has_inline_properties = obj.contains_key("properties") || obj.contains_key("type");
        if has_inline_properties {
            let mut builder = AllOfBuilder::new();
            // Build an object schema from the inline properties/required
            let mut obj_without_allof = obj.clone();
            obj_without_allof.remove("allOf");
            builder = builder.item(convert_json_object_to_utoipa(&obj_without_allof));
            for item in all_of {
                if let Ok(schema) = rmcp::schemars::Schema::try_from(item.clone()) {
                    builder = builder.item(convert_schemars_to_utoipa(schema));
                }
            }
            return RefOr::T(Schema::AllOf(builder.build()));
        }

        let mut builder = AllOfBuilder::new();
        for item in all_of {
            if let Ok(schema) = rmcp::schemars::Schema::try_from(item.clone()) {
                builder = builder.item(convert_schemars_to_utoipa(schema));
            }
        }
        return RefOr::T(Schema::AllOf(builder.build()));
    }

    if let Some(Value::Array(any_of)) = obj.get("anyOf") {
        let mut builder = AnyOfBuilder::new();
        for item in any_of {
            if let Ok(schema) = rmcp::schemars::Schema::try_from(item.clone()) {
                builder = builder.item(convert_schemars_to_utoipa(schema));
            }
        }
        return RefOr::T(Schema::AnyOf(builder.build()));
    }

    match obj.get("type") {
        Some(Value::String(type_str)) => convert_typed_schema(type_str, obj),
        Some(Value::Array(types)) => {
            let mut builder = AnyOfBuilder::new();
            for type_val in types {
                if let Value::String(type_str) = type_val {
                    builder = builder.item(convert_typed_schema(type_str, obj));
                }
            }
            RefOr::T(Schema::AnyOf(builder.build()))
        }
        None => RefOr::T(Schema::Object(ObjectBuilder::new().build())),
        _ => RefOr::T(Schema::Object(ObjectBuilder::new().build())),
    }
}

fn convert_typed_schema(
    type_str: &str,
    obj: &serde_json::Map<String, serde_json::Value>,
) -> RefOr<Schema> {
    use serde_json::Value;

    match type_str {
        "object" => {
            let mut object_builder = ObjectBuilder::new();

            if let Some(Value::Object(properties)) = obj.get("properties") {
                for (name, prop_value) in properties {
                    if let Ok(prop_schema) = rmcp::schemars::Schema::try_from(prop_value.clone()) {
                        let prop = convert_schemars_to_utoipa(prop_schema);
                        object_builder = object_builder.property(name, prop);
                    }
                }
            }

            if let Some(Value::Array(required)) = obj.get("required") {
                for req in required {
                    if let Value::String(field_name) = req {
                        object_builder = object_builder.required(field_name);
                    }
                }
            }

            if let Some(additional) = obj.get("additionalProperties") {
                match additional {
                    Value::Bool(false) => {
                        object_builder = object_builder
                            .additional_properties(Some(AdditionalProperties::FreeForm(false)));
                    }
                    Value::Bool(true) => {
                        object_builder = object_builder
                            .additional_properties(Some(AdditionalProperties::FreeForm(true)));
                    }
                    _ => {
                        if let Ok(schema) = rmcp::schemars::Schema::try_from(additional.clone()) {
                            let schema = convert_schemars_to_utoipa(schema);
                            object_builder = object_builder
                                .additional_properties(Some(AdditionalProperties::RefOr(schema)));
                        }
                    }
                }
            }

            RefOr::T(Schema::Object(object_builder.build()))
        }
        "array" => {
            let mut array_builder = ArrayBuilder::new();

            if let Some(items) = obj.get("items") {
                match items {
                    Value::Object(_) | Value::Bool(_) => {
                        if let Ok(item_schema) = rmcp::schemars::Schema::try_from(items.clone()) {
                            let item_schema = convert_schemars_to_utoipa(item_schema);
                            array_builder = array_builder.items(item_schema);
                        }
                    }
                    Value::Array(item_schemas) => {
                        let mut any_of = AnyOfBuilder::new();
                        for item in item_schemas {
                            if let Ok(schema) = rmcp::schemars::Schema::try_from(item.clone()) {
                                any_of = any_of.item(convert_schemars_to_utoipa(schema));
                            }
                        }
                        let any_of_schema = RefOr::T(Schema::AnyOf(any_of.build()));
                        array_builder = array_builder.items(any_of_schema);
                    }
                    _ => {}
                }
            }

            if let Some(Value::Number(min_items)) = obj.get("minItems") {
                if let Some(min) = min_items.as_u64() {
                    array_builder = array_builder.min_items(Some(min as usize));
                }
            }
            if let Some(Value::Number(max_items)) = obj.get("maxItems") {
                if let Some(max) = max_items.as_u64() {
                    array_builder = array_builder.max_items(Some(max as usize));
                }
            }

            RefOr::T(Schema::Array(array_builder.build()))
        }
        "string" => {
            let mut object_builder = ObjectBuilder::new().schema_type(SchemaType::String);

            if let Some(Value::Array(enum_values)) = obj.get("enum") {
                let values: Vec<serde_json::Value> = enum_values
                    .iter()
                    .filter_map(|v| {
                        if let Value::String(s) = v {
                            Some(Value::String(s.clone()))
                        } else {
                            None
                        }
                    })
                    .collect();
                if !values.is_empty() {
                    object_builder = object_builder.enum_values(Some(values));
                }
            }

            if let Some(Value::Number(min_length)) = obj.get("minLength") {
                if let Some(min) = min_length.as_u64() {
                    object_builder = object_builder.min_length(Some(min as usize));
                }
            }
            if let Some(Value::Number(max_length)) = obj.get("maxLength") {
                if let Some(max) = max_length.as_u64() {
                    object_builder = object_builder.max_length(Some(max as usize));
                }
            }
            if let Some(Value::String(pattern)) = obj.get("pattern") {
                object_builder = object_builder.pattern(Some(pattern.clone()));
            }
            if let Some(Value::String(format)) = obj.get("format") {
                object_builder = object_builder.format(Some(SchemaFormat::Custom(format.clone())));
            }

            RefOr::T(Schema::Object(object_builder.build()))
        }
        "number" => {
            let mut object_builder = ObjectBuilder::new().schema_type(SchemaType::Number);

            if let Some(Value::Number(minimum)) = obj.get("minimum") {
                if let Some(min) = minimum.as_f64() {
                    object_builder = object_builder.minimum(Some(min));
                }
            }
            if let Some(Value::Number(maximum)) = obj.get("maximum") {
                if let Some(max) = maximum.as_f64() {
                    object_builder = object_builder.maximum(Some(max));
                }
            }
            if let Some(Value::Number(exclusive_minimum)) = obj.get("exclusiveMinimum") {
                if let Some(min) = exclusive_minimum.as_f64() {
                    object_builder = object_builder.exclusive_minimum(Some(min));
                }
            }
            if let Some(Value::Number(exclusive_maximum)) = obj.get("exclusiveMaximum") {
                if let Some(max) = exclusive_maximum.as_f64() {
                    object_builder = object_builder.exclusive_maximum(Some(max));
                }
            }
            if let Some(Value::Number(multiple_of)) = obj.get("multipleOf") {
                if let Some(mult) = multiple_of.as_f64() {
                    object_builder = object_builder.multiple_of(Some(mult));
                }
            }

            RefOr::T(Schema::Object(object_builder.build()))
        }
        "integer" => {
            let mut object_builder = ObjectBuilder::new().schema_type(SchemaType::Integer);

            if let Some(Value::Number(minimum)) = obj.get("minimum") {
                if let Some(min) = minimum.as_f64() {
                    object_builder = object_builder.minimum(Some(min));
                }
            }
            if let Some(Value::Number(maximum)) = obj.get("maximum") {
                if let Some(max) = maximum.as_f64() {
                    object_builder = object_builder.maximum(Some(max));
                }
            }
            if let Some(Value::Number(exclusive_minimum)) = obj.get("exclusiveMinimum") {
                if let Some(min) = exclusive_minimum.as_f64() {
                    object_builder = object_builder.exclusive_minimum(Some(min));
                }
            }
            if let Some(Value::Number(exclusive_maximum)) = obj.get("exclusiveMaximum") {
                if let Some(max) = exclusive_maximum.as_f64() {
                    object_builder = object_builder.exclusive_maximum(Some(max));
                }
            }
            if let Some(Value::Number(multiple_of)) = obj.get("multipleOf") {
                if let Some(mult) = multiple_of.as_f64() {
                    object_builder = object_builder.multiple_of(Some(mult));
                }
            }

            RefOr::T(Schema::Object(object_builder.build()))
        }
        "boolean" => RefOr::T(Schema::Object(
            ObjectBuilder::new()
                .schema_type(SchemaType::Boolean)
                .build(),
        )),
        "null" => RefOr::T(Schema::Object(
            ObjectBuilder::new().schema_type(SchemaType::String).build(),
        )),
        _ => RefOr::T(Schema::Object(ObjectBuilder::new().build())),
    }
}

derive_utoipa!(Role as RoleSchema);
derive_utoipa!(Content as ContentSchema);
derive_utoipa!(RawContent as ContentBlockSchema => "ContentBlock");
derive_utoipa!(EmbeddedResource as EmbeddedResourceSchema);
derive_utoipa!(ImageContent as ImageContentSchema);
derive_utoipa!(TextContent as TextContentSchema);
derive_utoipa!(RawTextContent as RawTextContentSchema);
derive_utoipa!(RawImageContent as RawImageContentSchema);
derive_utoipa!(RawAudioContent as RawAudioContentSchema);
derive_utoipa!(RawEmbeddedResource as RawEmbeddedResourceSchema);
derive_utoipa!(RawResource as RawResourceSchema);
derive_utoipa!(Tool as ToolSchema);
derive_utoipa!(ToolAnnotations as ToolAnnotationsSchema);
derive_utoipa!(ToolExecution as ToolExecutionSchema);
derive_utoipa!(TaskSupport as TaskSupportSchema);
derive_utoipa!(Annotations as AnnotationsSchema);
derive_utoipa!(ResourceContents as ResourceContentsSchema);
derive_utoipa!(JsonObject as JsonObjectSchema);
derive_utoipa!(Icon as IconSchema);
derive_utoipa!(IconTheme as IconThemeSchema);

#[derive(OpenApi)]
#[openapi(
    paths(
        super::routes::status::status,
        super::routes::status::system_info,
        super::routes::status::diagnostics,
        super::routes::mcp_ui_proxy::mcp_ui_proxy,
        super::routes::config_management::backup_config,
        super::routes::config_management::recover_config,
        super::routes::config_management::validate_config,
        super::routes::config_management::init_config,
        super::routes::config_management::upsert_config,
        super::routes::config_management::remove_config,
        super::routes::config_management::read_config,
        super::routes::config_management::add_extension,
        super::routes::config_management::remove_extension,
        super::routes::config_management::get_extensions,
        super::routes::config_management::read_all_config,
        super::routes::config_management::providers,
        super::routes::config_management::get_provider_models,
        super::routes::config_management::get_slash_commands,
        super::routes::config_management::upsert_permissions,
        super::routes::config_management::create_custom_provider,
        super::routes::config_management::get_custom_provider,
        super::routes::config_management::update_custom_provider,
        super::routes::config_management::remove_custom_provider,
        super::routes::config_management::get_provider_catalog,
        super::routes::config_management::get_provider_catalog_template,
        super::routes::config_management::cleanup_provider_cache,
        super::routes::config_management::check_provider,
        super::routes::config_management::set_config_provider,
        super::routes::config_management::configure_provider_oauth,
        super::routes::config_management::get_canonical_model_info,
        super::routes::prompts::get_prompts,
        super::routes::prompts::get_prompt,
        super::routes::prompts::save_prompt,
        super::routes::prompts::reset_prompt,
        super::routes::agent::start_agent,
        super::routes::agent::resume_agent,
        super::routes::agent::stop_agent,
        super::routes::agent::restart_agent,
        super::routes::agent::update_working_dir,
        super::routes::agent::get_tools,
        super::routes::agent::read_resource,
        super::routes::agent::call_tool,
        super::routes::agent::list_apps,
        super::routes::agent::export_app,
        super::routes::agent::import_app,
        super::routes::agent::update_from_session,
        super::routes::agent::agent_add_extension,
        super::routes::agent::agent_remove_extension,
        super::routes::agent::update_agent_provider,
        super::routes::agent::update_session,
        super::routes::action_required::confirm_tool_action,
        super::routes::reply::reply,
        super::routes::session_events::session_events,
        super::routes::session_events::session_reply,
        super::routes::session_events::session_cancel,
        super::routes::session::list_sessions,
        super::routes::session::search_sessions,
        super::routes::session::get_session,
        super::routes::session::get_session_insights,
        super::routes::session::update_session_name,
        super::routes::session::delete_session,
        super::routes::session::export_session,
        super::routes::session::import_session,
        super::routes::session::update_session_user_recipe_values,
        super::routes::session::fork_session,
        super::routes::session::get_session_extensions,
        super::routes::schedule::create_schedule,
        super::routes::schedule::list_schedules,
        super::routes::schedule::delete_schedule,
        super::routes::schedule::update_schedule,
        super::routes::schedule::run_now_handler,
        super::routes::schedule::pause_schedule,
        super::routes::schedule::unpause_schedule,
        super::routes::schedule::kill_running_job,
        super::routes::schedule::inspect_running_job,
        super::routes::schedule::sessions_handler,
        super::routes::recipe::create_recipe,
        super::routes::recipe::encode_recipe,
        super::routes::recipe::decode_recipe,
        super::routes::recipe::scan_recipe,
        super::routes::recipe::list_recipes,
        super::routes::recipe::delete_recipe,
        super::routes::recipe::schedule_recipe,
        super::routes::recipe::set_recipe_slash_command,
        super::routes::recipe::save_recipe,
        super::routes::recipe::parse_recipe,
        super::routes::recipe::recipe_to_yaml,
        super::routes::setup::start_openrouter_setup,
        super::routes::setup::start_tetrate_setup,
        super::routes::setup::start_nanogpt_setup,
        super::routes::tunnel::start_tunnel,
        super::routes::tunnel::stop_tunnel,
        super::routes::tunnel::get_tunnel_status,
        super::routes::telemetry::send_telemetry_event,
        super::routes::dictation::transcribe_dictation,
        super::routes::dictation::get_dictation_config,
        super::routes::features::get_features,
    ),
    components(schemas(
        super::routes::config_management::UpsertConfigQuery,
        super::routes::config_management::ConfigKeyQuery,
        super::routes::config_management::ConfigResponse,
        super::routes::config_management::ProvidersResponse,
        super::routes::config_management::ProviderDetails,
        super::routes::config_management::SlashCommandsResponse,
        super::routes::config_management::SlashCommand,
        super::routes::config_management::CommandType,
        super::routes::config_management::ExtensionResponse,
        super::routes::config_management::ExtensionQuery,
        super::routes::config_management::ToolPermission,
        super::routes::config_management::UpsertPermissionsQuery,
        super::routes::config_management::UpdateCustomProviderRequest,
        goose::providers::catalog::ProviderCatalogEntry,
        goose::providers::catalog::ProviderTemplate,
        goose::providers::catalog::ModelTemplate,
        goose::providers::catalog::ModelCapabilities,
        super::routes::config_management::CreateCustomProviderResponse,
        super::routes::config_management::CheckProviderRequest,
        super::routes::config_management::SetProviderRequest,
        super::routes::config_management::ModelInfoQuery,
        super::routes::config_management::ModelInfoResponse,
        super::routes::config_management::ModelInfoData,
        super::routes::prompts::PromptsListResponse,
        super::routes::prompts::PromptContentResponse,
        super::routes::prompts::SavePromptRequest,
        goose::prompt_template::Template,
        super::routes::action_required::ConfirmToolActionRequest,
        super::routes::reply::ChatRequest,
        super::routes::session_events::SessionReplyRequest,
        super::routes::session_events::SessionReplyResponse,
        super::routes::session_events::CancelRequest,
        super::routes::session::ImportSessionRequest,
        super::routes::session::SessionListResponse,
        super::routes::session::UpdateSessionNameRequest,
        super::routes::session::UpdateSessionUserRecipeValuesRequest,
        super::routes::session::UpdateSessionUserRecipeValuesResponse,
        super::routes::session::ForkRequest,
        super::routes::session::ForkResponse,
        super::routes::session::SessionExtensionsResponse,
        Message,
        MessageContent,
        MessageMetadata,
        TokenState,
        ContentSchema,
        EmbeddedResourceSchema,
        ImageContentSchema,
        AnnotationsSchema,
        TextContentSchema,
        RawTextContentSchema,
        RawImageContentSchema,
        RawAudioContentSchema,
        RawEmbeddedResourceSchema,
        RawResourceSchema,
        ToolResponse,
        ToolRequest,
        ToolConfirmationRequest,
        ActionRequired,
        ActionRequiredData,
        ThinkingContent,
        RedactedThinkingContent,
        FrontendToolRequest,
        ResourceContentsSchema,
        SystemNotificationType,
        SystemNotificationContent,
        MessageEvent,
        JsonObjectSchema,
        RoleSchema,
        ProviderMetadata,
        ProviderType,
        LoadedProvider,
        ProviderEngine,
        DeclarativeProviderConfig,
        EnvVarConfig,
        ExtensionEntry,
        ExtensionConfig,
        ConfigKey,
        Envs,
        RecipeManifest,
        ToolSchema,
        ToolAnnotationsSchema,
        ToolExecutionSchema,
        TaskSupportSchema,
        ToolInfo,
        PermissionLevel,
        Permission,
        PrincipalType,
        ModelInfo,
        ModelConfig,
        Session,
        goose::config::goose_mode::GooseMode,
        SessionInsights,
        SessionType,
        SystemInfo,
        Conversation,
        IconSchema,
        IconThemeSchema,
        goose::session::extension_data::ExtensionData,
        super::routes::schedule::CreateScheduleRequest,
        super::routes::schedule::UpdateScheduleRequest,
        super::routes::schedule::KillJobResponse,
        super::routes::schedule::InspectJobResponse,
        goose::scheduler::ScheduledJob,
        super::routes::schedule::RunNowResponse,
        super::routes::schedule::ListSchedulesResponse,
        super::routes::schedule::SessionsQuery,
        super::routes::schedule::SessionDisplayInfo,
        super::routes::recipe::CreateRecipeRequest,
        super::routes::recipe::AuthorRequest,
        super::routes::recipe::CreateRecipeResponse,
        super::routes::recipe::EncodeRecipeRequest,
        super::routes::recipe::EncodeRecipeResponse,
        super::routes::recipe::DecodeRecipeRequest,
        super::routes::recipe::DecodeRecipeResponse,
        super::routes::recipe::ScanRecipeRequest,
        super::routes::recipe::ScanRecipeResponse,
        super::routes::recipe::ListRecipeResponse,
        super::routes::recipe::ScheduleRecipeRequest,
        super::routes::recipe::SetSlashCommandRequest,
        super::routes::recipe::DeleteRecipeRequest,
        super::routes::recipe::SaveRecipeRequest,
        super::routes::recipe::SaveRecipeResponse,
        super::routes::errors::ErrorResponse,
        super::routes::recipe::ParseRecipeRequest,
        super::routes::recipe::ParseRecipeResponse,
        super::routes::recipe::RecipeToYamlRequest,
        super::routes::recipe::RecipeToYamlResponse,
        goose::recipe::Recipe,
        goose::recipe::Author,
        goose::recipe::Settings,
        goose::recipe::RecipeParameter,
        goose::recipe::RecipeParameterInputType,
        goose::recipe::RecipeParameterRequirement,
        goose::recipe::Response,
        goose::recipe::SubRecipe,
        goose::agents::types::RetryConfig,
        goose::agents::types::SuccessCheck,
        super::routes::agent::UpdateProviderRequest,
        super::routes::agent::UpdateSessionRequest,
        super::routes::agent::GetToolsQuery,
        super::routes::agent::ReadResourceRequest,
        super::routes::agent::ReadResourceResponse,
        super::routes::agent::CallToolRequest,
        super::routes::agent::CallToolResponse,
        ContentBlockSchema,
        super::routes::agent::ListAppsRequest,
        super::routes::agent::ListAppsResponse,
        super::routes::agent::ImportAppRequest,
        super::routes::agent::ImportAppResponse,
        super::routes::agent::StartAgentRequest,
        super::routes::agent::ResumeAgentRequest,
        super::routes::agent::StopAgentRequest,
        super::routes::agent::RestartAgentRequest,
        super::routes::agent::UpdateWorkingDirRequest,
        super::routes::agent::UpdateFromSessionRequest,
        super::routes::agent::AddExtensionRequest,
        super::routes::agent::RemoveExtensionRequest,
        super::routes::agent::ResumeAgentResponse,
        super::routes::agent::RestartAgentResponse,
        goose::agents::ExtensionLoadResult,
        super::routes::setup::SetupResponse,
        super::tunnel::TunnelInfo,
        super::tunnel::TunnelState,
        super::routes::telemetry::TelemetryEventRequest,
        goose::goose_apps::GooseApp,
        goose::goose_apps::WindowProps,
        goose::goose_apps::McpAppResource,
        goose::goose_apps::CspMetadata,
        goose::goose_apps::PermissionsMetadata,
        goose::goose_apps::UiMetadata,
        goose::goose_apps::ResourceMetadata,
        super::routes::dictation::TranscribeRequest,
        super::routes::dictation::TranscribeResponse,
        goose::dictation::providers::DictationProvider,
        super::routes::dictation::DictationProviderStatus,
        super::routes::features::FeaturesResponse,
        DownloadProgress,
        DownloadStatus,
    ))
)]
pub struct ApiDoc;

#[cfg(feature = "local-inference")]
#[derive(OpenApi)]
#[openapi(
    paths(
        super::routes::dictation::list_models,
        super::routes::dictation::download_model,
        super::routes::dictation::get_download_progress,
        super::routes::dictation::cancel_download,
        super::routes::dictation::delete_model,
        super::routes::local_inference::list_local_models,
        super::routes::local_inference::sync_featured_models,
        super::routes::local_inference::search_hf_models,
        super::routes::local_inference::get_repo_files,
        super::routes::local_inference::download_hf_model,
        super::routes::local_inference::get_local_model_download_progress,
        super::routes::local_inference::cancel_local_model_download,
        super::routes::local_inference::delete_local_model,
        super::routes::local_inference::get_model_settings,
        super::routes::local_inference::update_model_settings,
    ),
    components(schemas(
        super::routes::dictation::WhisperModelResponse,
        super::routes::local_inference::LocalModelResponse,
        super::routes::local_inference::ModelDownloadStatus,
        super::routes::local_inference::DownloadModelRequest,
        goose::providers::local_inference::hf_models::HfModelInfo,
        goose::providers::local_inference::hf_models::HfGgufFile,
        goose::providers::local_inference::hf_models::HfQuantVariant,
        super::routes::local_inference::RepoVariantsResponse,
        goose::providers::local_inference::local_model_registry::ModelSettings,
        goose::providers::local_inference::local_model_registry::SamplingConfig,
    ))
)]
pub struct LocalInferenceApiDoc;

#[allow(dead_code)] // Used by generate_schema binary
pub fn generate_schema() -> String {
    #[allow(unused_mut)]
    let mut api_doc = ApiDoc::openapi();

    #[cfg(feature = "local-inference")]
    api_doc.merge(LocalInferenceApiDoc::openapi());

    serde_json::to_string_pretty(&api_doc).unwrap()
}
