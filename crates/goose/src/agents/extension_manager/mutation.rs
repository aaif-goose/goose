use crate::agents::extension_config::{name_to_key, ExtensionError, ExtensionResult};
use crate::agents::extension_runtime::tool_execution::ToolCallResult;
use futures::FutureExt;
use rmcp::model::{CallToolResult, ErrorCode, ErrorData, MetaObject};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "action", rename_all = "lowercase")]
pub enum ExtensionMutation {
    Enable { name: String },
    Disable { name: String },
}

pub(super) const EXTENSION_MUTATION_META_KEY: &str = "goose_extension_mutation";

impl ExtensionMutation {
    pub fn attach(self, result: &mut CallToolResult) {
        let mut meta = result.meta.take().map(|m| m.0).unwrap_or_default();
        meta.insert(
            EXTENSION_MUTATION_META_KEY.to_string(),
            serde_json::to_value(self).expect("mutation serializes"),
        );
        result.meta = Some(MetaObject(meta));
    }

    pub fn take(result: &mut CallToolResult) -> Option<Self> {
        let meta = result.meta.as_mut()?;
        let value = meta.0.remove(EXTENSION_MUTATION_META_KEY)?;
        if meta.0.is_empty() {
            result.meta = None;
        }
        serde_json::from_value(value).ok()
    }
}

async fn apply(
    manager: &Arc<crate::agents::extension_runtime::ExtensionManager>,
    mutation: ExtensionMutation,
    session_id: &str,
    context: &crate::agents::platform_extensions::PlatformExtensionContext,
) -> ExtensionResult<()> {
    let adapter = super::ExtensionManager {
        manager: Arc::clone(manager),
        context: context.clone(),
    };
    match mutation {
        ExtensionMutation::Enable { name } => {
            let config = crate::config::get_extension_by_name(&name).ok_or_else(|| {
                ExtensionError::ConfigError(format!("Extension '{}' not found", name))
            })?;
            adapter.enable(session_id, config).await
        }
        ExtensionMutation::Disable { name } => {
            adapter.disable(session_id, &name_to_key(&name)).await?;
            Ok(())
        }
    }
}

pub(crate) fn applying_mutation(
    manager: &Arc<crate::agents::extension_runtime::ExtensionManager>,
    result: ToolCallResult,
    session_id: &str,
    context: &crate::agents::platform_extensions::PlatformExtensionContext,
) -> ToolCallResult {
    let manager = Arc::clone(manager);
    let session_id = session_id.to_string();
    let context = context.clone();
    let inner = result.result;
    ToolCallResult {
        result: Box::new(
            async move {
                let mut result = inner.await?;
                if let Some(mutation) = ExtensionMutation::take(&mut result) {
                    apply(&manager, mutation, &session_id, &context)
                        .await
                        .map_err(|e| {
                            ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
                        })?;
                }
                Ok(result)
            }
            .boxed(),
        ),
        ..result
    }
}

impl super::ExtensionManager {
    pub async fn apply(
        &self,
        mutation: ExtensionMutation,
        session_id: &str,
    ) -> ExtensionResult<()> {
        apply(&self.manager, mutation, session_id, &self.context).await
    }

    pub fn applying_mutation(&self, result: ToolCallResult, session_id: &str) -> ToolCallResult {
        applying_mutation(&self.manager, result, session_id, &self.context)
    }
}
