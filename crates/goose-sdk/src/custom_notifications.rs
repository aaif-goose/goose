use crate::custom_requests::CustomMethodSchema;
use agent_client_protocol::{JsonRpcMessage, JsonRpcNotification};
use schemars::{JsonSchema, SchemaGenerator};
use serde::{Deserialize, Serialize};

/// Goose-custom session update notification — a parallel to ACP's
/// `session/update` carrying goose-specific update variants.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, JsonRpcNotification)]
#[notification(method = "_goose/session/update")]
#[serde(rename_all = "camelCase")]
pub struct GooseSessionNotification {
    pub session_id: String,
    pub update: GooseSessionUpdate,
}

/// Discriminated union of goose-specific session update payloads.
/// Variant tag matches ACP's convention (`sessionUpdate: "<snake_case>"`).
///
/// `discriminator.mapping` is what makes TS codegen (`@hey-api/openapi-ts`)
/// emit the correct snake_case tag value even when this enum has a single
/// variant. Add a mapping entry per variant.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "sessionUpdate", rename_all = "snake_case")]
#[schemars(extend("discriminator" = {
    "propertyName": "sessionUpdate",
    "mapping": {
        "usage_update": "#/$defs/SessionUsageUpdate"
    }
}))]
pub enum GooseSessionUpdate {
    UsageUpdate(SessionUsageUpdate),
}

impl Default for GooseSessionUpdate {
    fn default() -> Self {
        GooseSessionUpdate::UsageUpdate(SessionUsageUpdate::default())
    }
}

/// Streaming context-window usage update for a session.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionUsageUpdate {
    pub used: u64,
    pub context_limit: u64,
    pub accumulated_input_tokens: u64,
    pub accumulated_output_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accumulated_cost: Option<f64>,
}

fn notification_schema<T>(generator: &mut SchemaGenerator) -> CustomMethodSchema
where
    T: Default + JsonRpcMessage + JsonSchema,
{
    let dummy = T::default();
    let type_name = std::any::type_name::<T>()
        .rsplit("::")
        .next()
        .unwrap_or(std::any::type_name::<T>())
        .to_string();
    CustomMethodSchema {
        method: dummy.method().to_string(),
        params_schema: Some(generator.subschema_for::<T>()),
        params_type_name: Some(type_name),
        response_schema: None,
        response_type_name: None,
    }
}

/// Schemas for every goose-custom outbound notification. To register a new
/// notification, define the struct above (with `JsonRpcNotification` +
/// `Default`) and add one line below.
pub fn custom_notification_schemas(generator: &mut SchemaGenerator) -> Vec<CustomMethodSchema> {
    vec![notification_schema::<GooseSessionNotification>(generator)]
}
