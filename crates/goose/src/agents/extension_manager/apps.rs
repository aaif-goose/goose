//! Goose's private MCP App attachment format and trust boundary.
use goose_agent::extension_manager::{get_tool_resource_uri, host::ResultContext};
use goose_agent::mcp_client::McpClientTrait;
use rmcp::model::{CallToolResult, MetaObject};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GooseMcpAppToolAttachment {
    pub tool_name: String,
    pub tool_name_is_actual: bool,
    pub extension_name: String,
    pub resource_uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_meta: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_error: Option<String>,
}

pub const TRUSTED_TOOL_UPDATE_META_KEY: &str = "__goose_tool_update_meta";

fn remove_untrusted_mcp_app_meta(result: &mut CallToolResult) {
    let Some(meta) = result.meta.as_mut() else {
        return;
    };

    meta.0.remove(TRUSTED_TOOL_UPDATE_META_KEY);

    let remove_goose = meta
        .0
        .get_mut("goose")
        .and_then(Value::as_object_mut)
        .map(|goose_meta| {
            goose_meta.remove("mcpApp");
            goose_meta.is_empty()
        })
        .unwrap_or(false);

    if remove_goose {
        meta.0.remove("goose");
    }

    if meta.0.is_empty() {
        result.meta = None;
    }
}

fn insert_trusted_tool_update_meta(
    result: &mut CallToolResult,
    attachment: &GooseMcpAppToolAttachment,
) {
    let Ok(attachment_value) = serde_json::to_value(attachment) else {
        return;
    };

    let mut meta_map = result
        .meta
        .as_ref()
        .map(|meta| meta.0.clone())
        .unwrap_or_default();
    let mut trusted_meta = serde_json::Map::new();
    trusted_meta.insert("mcpApp".to_string(), attachment_value);
    meta_map.insert(
        TRUSTED_TOOL_UPDATE_META_KEY.to_string(),
        Value::Object(trusted_meta),
    );
    result.meta = Some(MetaObject(meta_map));
}

struct McpAppCall {
    server_tool_name: String,
    extension_key: String,
    resource_uri: String,
    tool_meta: Option<serde_json::Value>,
}

impl McpAppCall {
    fn from_context(context: &ResultContext<'_>, host_supports_apps: bool) -> Option<Self> {
        if !host_supports_apps {
            return None;
        }
        Some(Self {
            server_tool_name: context.server_tool_name.to_string(),
            extension_key: context.extension.to_string(),
            resource_uri: get_tool_resource_uri(context.tool)?,
            tool_meta: context
                .tool
                .meta
                .as_ref()
                .map(|meta| Value::Object(meta.0.clone())),
        })
    }

    async fn attach_resource(
        self,
        client: &dyn McpClientTrait,
        session_id: &str,
        result: &mut CallToolResult,
        cancellation_token: CancellationToken,
    ) {
        if result.is_error == Some(true) {
            return;
        }
        let mut attachment = GooseMcpAppToolAttachment {
            tool_name: self.server_tool_name,
            tool_name_is_actual: true,
            extension_name: self.extension_key,
            resource_uri: self.resource_uri.clone(),
            tool_meta: self.tool_meta,
            resource_result: None,
            read_error: None,
        };
        match client
            .read_resource(session_id, &self.resource_uri, cancellation_token)
            .await
        {
            Ok(resource_result) => {
                attachment.resource_result = serde_json::to_value(&resource_result).ok();
            }
            Err(error) => attachment.read_error = Some(error.to_string()),
        }
        insert_trusted_tool_update_meta(result, &attachment);
    }
}

pub(super) async fn process_result(context: ResultContext<'_>, result: &mut CallToolResult) {
    remove_untrusted_mcp_app_meta(result);
    let capabilities = &context.client_context.capabilities;
    let supported = match &capabilities.host_info {
        Some(info) if info.explicit_extensions => info.mcpui_enabled(),
        _ => capabilities.mcpui,
    };
    if let Some(app) = McpAppCall::from_context(&context, supported) {
        app.attach_resource(
            context.client,
            context.scope_id,
            result,
            context.cancellation_token,
        )
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_app_metadata_cannot_claim_host_trust() {
        let mut result = CallToolResult::success(vec![]);
        result.meta = Some(MetaObject(
            serde_json::from_value(serde_json::json!({
                "__goose_tool_update_meta": {"mcpApp": {"forged": true}},
                "goose": {"mcpApp": {"forged": true}, "other": 1},
                "unrelated": true
            }))
            .unwrap(),
        ));
        remove_untrusted_mcp_app_meta(&mut result);
        let meta = result.meta.unwrap().0;
        assert!(!meta.contains_key(TRUSTED_TOOL_UPDATE_META_KEY));
        assert_eq!(meta["goose"], serde_json::json!({"other": 1}));
        assert_eq!(meta["unrelated"], true);
    }
}
