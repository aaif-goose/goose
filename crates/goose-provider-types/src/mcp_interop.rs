//! Optional conversions at the desktop MCP boundary. No transport is exposed by the core.
macro_rules! bridge {
    ($($name:ident),* $(,)?) => { $(
        impl TryFrom<rmcp::model::$name> for crate::data::$name {
            type Error = serde_json::Error;
            fn try_from(value: rmcp::model::$name) -> Result<Self, Self::Error> {
                serde_json::from_value(serde_json::to_value(value)?)
            }
        }
        impl TryFrom<crate::data::$name> for rmcp::model::$name {
            type Error = serde_json::Error;
            fn try_from(value: crate::data::$name) -> Result<Self, Self::Error> {
                serde_json::from_value(serde_json::to_value(value)?)
            }
        }
    )* };
}
bridge!(
    Tool,
    ToolAnnotations,
    CallToolRequestParams,
    CallToolResult,
    ContentBlock,
    TextContent,
    ImageContent,
    AudioContent,
    ResourceContents,
    Resource,
    EmbeddedResource,
    Annotations,
    MetaObject,
    ElicitationAction,
    PromptMessage,
    Icon,
);

impl From<rmcp::model::ErrorData> for crate::data::ErrorData {
    fn from(value: rmcp::model::ErrorData) -> Self {
        Self {
            code: crate::data::ErrorCode(value.code.0),
            message: value.message,
            data: value.data,
        }
    }
}
impl From<crate::data::ErrorData> for rmcp::model::ErrorData {
    fn from(value: crate::data::ErrorData) -> Self {
        Self::new(
            rmcp::model::ErrorCode(value.code.0),
            value.message,
            value.data,
        )
    }
}
impl From<rmcp::model::Role> for crate::data::Role {
    fn from(value: rmcp::model::Role) -> Self {
        match value {
            rmcp::model::Role::User => Self::User,
            rmcp::model::Role::Assistant => Self::Assistant,
        }
    }
}
impl From<crate::data::Role> for rmcp::model::Role {
    fn from(value: crate::data::Role) -> Self {
        match value {
            crate::data::Role::User => Self::User,
            crate::data::Role::Assistant => Self::Assistant,
        }
    }
}
