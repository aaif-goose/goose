//! MockClient is a mock implementation of the McpClientTrait for testing purposes.
//! add a tool you want to have around and then add the client to the extension router

use goose::agents::mcp_client::{Error, McpClientTrait};
use rmcp::{
    model::{
        CallToolResult, Content, ErrorData, GetPromptResult, ListPromptsResult,
        ListResourcesResult, ListToolsResult, ReadResourceResult, ServerNotification, Tool,
    },
    object,
};
use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::mpsc::{self, Receiver};
use tokio_util::sync::CancellationToken;

type Handler = Box<dyn Fn(&Value) -> Result<Vec<Content>, ErrorData> + Send + Sync>;

pub struct MockClient {
    tools: HashMap<String, Tool>,
    handlers: HashMap<String, Handler>,
}

impl MockClient {
    pub(crate) fn new() -> Self {
        Self {
            tools: HashMap::new(),
            handlers: HashMap::new(),
        }
    }

    pub(crate) fn add_tool<F>(mut self, tool: Tool, handler: F) -> Self
    where
        F: Fn(&Value) -> Result<Vec<Content>, ErrorData> + Send + Sync + 'static,
    {
        let tool_name = tool.name.to_string();
        self.tools.insert(tool_name.clone(), tool);
        self.handlers.insert(tool_name, Box::new(handler));
        self
    }
}

#[async_trait::async_trait]
impl McpClientTrait for MockClient {
    async fn list_resources(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancel_token: CancellationToken,
    ) -> Result<ListResourcesResult, Error> {
        Ok(ListResourcesResult {
            resources: vec![],
            next_cursor: None,
            meta: None,
        })
    }

    fn get_info(&self) -> std::option::Option<&rmcp::model::InitializeResult> {
        None
    }

    async fn read_resource(
        &self,
        _session_id: &str,
        _uri: &str,
        _cancel_token: CancellationToken,
    ) -> Result<ReadResourceResult, Error> {
        Err(Error::UnexpectedResponse)
    }

    async fn list_tools(
        &self,
        _session_id: &str,
        _: Option<String>,
        _cancel_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        let rmcp_tools: Vec<rmcp::model::Tool> = self
            .tools
            .values()
            .map(|tool| {
                rmcp::model::Tool::new(
                    tool.name.to_string(),
                    tool.description.clone().unwrap_or_default(),
                    tool.input_schema.clone(),
                )
            })
            .collect();

        Ok(ListToolsResult {
            tools: rmcp_tools,
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        _ctx: &goose::agents::ToolCallContext,
        name: &str,
        arguments: Option<serde_json::Map<String, Value>>,
        _cancel_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        if let Some(handler) = self.handlers.get(name) {
            match handler(&Value::Object(arguments.unwrap_or_default())) {
                Ok(content) => Ok(CallToolResult::success(content)),
                Err(_e) => Err(Error::UnexpectedResponse),
            }
        } else {
            Err(Error::UnexpectedResponse)
        }
    }

    async fn list_prompts(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancel_token: CancellationToken,
    ) -> Result<ListPromptsResult, Error> {
        Ok(ListPromptsResult {
            prompts: vec![],
            next_cursor: None,
            meta: None,
        })
    }

    async fn get_prompt(
        &self,
        _session_id: &str,
        _name: &str,
        _arguments: Value,
        _cancel_token: CancellationToken,
    ) -> Result<GetPromptResult, Error> {
        Err(Error::UnexpectedResponse)
    }

    async fn subscribe(&self) -> Receiver<ServerNotification> {
        mpsc::channel(1).1
    }
}

pub const WEATHER_TYPE: &str = "cloudy";

pub fn weather_client() -> MockClient {
    let weather_tool = Tool::new(
        "get_weather",
        "Get the weather for a location",
        object!({
            "type": "object",
            "required": ["location"],
            "properties": {
                "location": {
                    "type": "string",
                    "description": "The city and state, e.g. San Francisco, CA"
                }
            }
        }),
    );

    let mock_client = MockClient::new().add_tool(weather_tool, |args| {
        let location = args
            .get("location")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown location");

        Ok(vec![Content::text(format!(
            "The weather in {} is {} and 18°C",
            location, WEATHER_TYPE
        ))])
    });
    mock_client
}
