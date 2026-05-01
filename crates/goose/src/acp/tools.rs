use rmcp::{
    model::{CallToolResult, Meta},
    object,
};

const ACP_AWARE_META_KEY: &str = "_goose/acp-aware";

pub trait AcpAwareToolMeta {
    fn with_acp_aware_meta(self) -> Self;
    fn is_acp_aware(&self) -> bool;
}

impl AcpAwareToolMeta for CallToolResult {
    fn with_acp_aware_meta(self) -> Self {
        self.with_meta(Some(Meta(object!({ACP_AWARE_META_KEY: true}))))
    }

    fn is_acp_aware(&self) -> bool {
        self.meta
            .as_ref()
            .and_then(|meta| meta.get(ACP_AWARE_META_KEY))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }
}
