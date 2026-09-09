use include_dir::{include_dir, Dir};
use indoc::formatdoc;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        Annotations, CallToolResult, ContentBlock, ErrorCode, ErrorData, Implementation,
        InitializeResult, Role, ServerCapabilities, ServerInfo, TextContent,
    },
    schemars::JsonSchema,
    tool, tool_handler, tool_router, ServerHandler,
};
use serde::{Deserialize, Serialize};

static TUTORIALS_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/src/tutorial/tutorials");

/// Parameters for the load_tutorial tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct LoadTutorialParams {
    /// Name of the tutorial to load, e.g. 'getting-started' or 'developer-mcp'
    pub name: String,
}

/// Tutorial MCP Server using official RMCP SDK
#[derive(Clone)]
pub struct TutorialServer {
    tool_router: ToolRouter<Self>,
    instructions: String,
}

impl Default for TutorialServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router(router = tool_router)]
impl TutorialServer {
    pub fn new() -> Self {
        // Get base instructions and available tutorials
        let available_tutorials = Self::get_available_tutorials();

        let instructions = formatdoc! {r#"
            Because the tutorial extension is enabled, be aware that the user may be new to using goose
            or looking for help with specific features. Proactively offer relevant tutorials when appropriate.

            Available tutorials:
            {tutorials}

            The specific content of the tutorial are available in by running load_tutorial.
            To run through a tutorial, make sure to be interactive with the user. Don't run more than
            a few related tool calls in a row. Make sure to prompt the user for understanding and participation.

            **Important**: Make sure that you provide guidance or info *before* you run commands, as the command will
            run immediately for the user. For example while running a game tutorial, let the user know what to expect
            before you run a command to start the game itself.
            "#,
            tutorials=available_tutorials,
        };

        Self {
            tool_router: Self::tool_router(),
            instructions,
        }
    }

    fn get_available_tutorials() -> String {
        let mut tutorials = String::new();
        for file in TUTORIALS_DIR.files() {
            // Use first line for additional context
            let first_line = file
                .contents_utf8()
                .and_then(|s| s.lines().next().map(|line| line.to_string()))
                .unwrap_or_else(String::new);

            if let Some(name) = file.path().file_stem() {
                tutorials.push_str(&format!("- {}: {}\n", name.to_string_lossy(), first_line));
            }
        }
        tutorials
    }

    /// Load a specific tutorial by name.
    /// The tutorial will be returned as markdown content that provides step by step instructions.
    #[tool(
        name = "load_tutorial",
        description = "Load a specific tutorial by name. The tutorial will be returned as markdown content that provides step by step instructions."
    )]
    pub async fn load_tutorial(
        &self,
        params: Parameters<LoadTutorialParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let name = &params.name;

        let file_name = format!("{}.md", name);
        let file = TUTORIALS_DIR.get_file(&file_name).ok_or(ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!("Could not locate tutorial '{}'", name),
            None,
        ))?;
        let content = String::from_utf8_lossy(file.contents()).into_owned();

        Ok(CallToolResult::success(vec![ContentBlock::Text(
            TextContent::new(content)
                .with_annotations(Annotations::default().with_audience(vec![Role::Assistant])),
        )]))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for TutorialServer {
    fn get_info(&self) -> ServerInfo {
        InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "goose-tutorial",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(self.instructions.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::handler::server::wrapper::Parameters;

    #[tokio::test]
    async fn test_tutorial_server_creation() {
        let server = TutorialServer::new();
        assert!(!server.instructions.is_empty());
    }

    #[tokio::test]
    async fn test_get_info() {
        let server = TutorialServer::new();
        let info = server.get_info();

        assert_eq!(info.server_info.name, "goose-tutorial");
        assert!(info.instructions.is_some());
        assert!(info
            .instructions
            .unwrap()
            .contains("tutorial extension is enabled"));
    }

    #[tokio::test]
    async fn test_get_available_tutorials() {
        let tutorials = TutorialServer::get_available_tutorials();
        assert!(!tutorials.is_empty());
        // Check for known tutorials that actually exist
        assert!(tutorials.contains("build-mcp-extension") || tutorials.contains("first-game"));
    }

    #[tokio::test]
    async fn test_load_tutorial_success() {
        let server = TutorialServer::new();

        // Try to load a tutorial that should exist (build-mcp-extension)
        let params = LoadTutorialParams {
            name: "build-mcp-extension".to_string(),
        };

        let result = server.load_tutorial(Parameters(params)).await;
        assert!(result.is_ok());

        let call_result = result.unwrap();
        assert!(!call_result.content.is_empty());

        let first_content = &call_result.content[0];
        let text_content = first_content.as_text().expect("content should be text");
        let audience = text_content
            .annotations
            .as_ref()
            .and_then(|annotations| annotations.audience.as_ref());
        assert_eq!(audience, Some(&vec![Role::Assistant]));
    }

    #[tokio::test]
    async fn test_load_tutorial_not_found() {
        let server = TutorialServer::new();

        let params = LoadTutorialParams {
            name: "nonexistent-tutorial".to_string(),
        };

        let result = server.load_tutorial(Parameters(params)).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.code, ErrorCode::INTERNAL_ERROR);
        assert!(err.message.contains("Could not locate tutorial"));
    }

    #[tokio::test]
    async fn test_instructions_contain_available_tutorials() {
        let server = TutorialServer::new();
        let info = server.get_info();

        let instructions = info.instructions.unwrap();
        assert!(instructions.contains("Available tutorials:"));

        // Check that the instructions contain the tutorial list
        let available_tutorials = TutorialServer::get_available_tutorials();
        // The instructions should contain at least some part of the tutorial list
        assert!(available_tutorials
            .lines()
            .any(|line| instructions.contains(line)));
    }
}
