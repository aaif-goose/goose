pub mod format;
pub mod graph;
pub mod languages;
pub mod parser;

use crate::agents::extension::PlatformExtensionContext;
use crate::agents::mcp_client::{Error, McpClientTrait};
use crate::agents::tool_execution::ToolCallContext;
use anyhow::Result;
use async_trait::async_trait;
use ignore::WalkBuilder;
use indoc::indoc;
use parser::{FileAnalysis, Parser};
use rayon::prelude::*;
use rmcp::model::{
    CallToolResult, Content, Implementation, InitializeResult, JsonObject, ListToolsResult,
    ServerCapabilities, Tool, ToolAnnotations,
};
use schemars::{schema_for, JsonSchema};
use serde::Deserialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

pub static EXTENSION_NAME: &str = "analyze";

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AnalyzeParams {
    /// File or directory path to analyze
    pub path: String,
    /// Symbol name to focus on (triggers call graph mode)
    #[serde(default)]
    pub focus: Option<String>,
    /// Directory recursion depth limit (default 3, 0=unlimited). Also limits focus scan depth.
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,
    /// Call graph traversal depth (default 2, 0=definitions only)
    #[serde(default = "default_follow_depth")]
    pub follow_depth: u32,
    /// Allow large outputs without size warning
    #[serde(default)]
    pub force: bool,
}

fn default_max_depth() -> u32 {
    3
}
fn default_follow_depth() -> u32 {
    2
}

pub struct AnalyzeClient {
    info: InitializeResult,
}

impl AnalyzeClient {
    pub fn new(_context: PlatformExtensionContext) -> Result<Self> {
        let info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(EXTENSION_NAME, "1.0.0").with_title("Analyze"))
            .with_instructions(indoc! {"
            Analyze code structure using tree-sitter AST parsing. Three auto-selected modes:
            - Directory path → structure overview (file tree with function/class counts)
            - File path → semantic details (functions, classes, imports, call counts)
            - Any path + focus parameter → symbol call graph (incoming/outgoing chains)

            For large codebases, delegate analysis to a subagent and retain only the summary.
        "});

        Ok(Self { info })
    }

    fn schema<T: JsonSchema>() -> JsonObject {
        serde_json::to_value(schema_for!(T))
            .expect("schema serialization should succeed")
            .as_object()
            .expect("schema should serialize to an object")
            .clone()
    }

    fn parse_args<T: serde::de::DeserializeOwned>(
        arguments: Option<JsonObject>,
    ) -> Result<T, String> {
        let value = arguments
            .map(Value::Object)
            .ok_or_else(|| "Missing arguments".to_string())?;
        serde_json::from_value(value).map_err(|e| format!("Failed to parse arguments: {e}"))
    }

    fn resolve_path(path: &str, working_dir: Option<&Path>) -> PathBuf {
        let p = PathBuf::from(path);
        if p.is_absolute() {
            p
        } else if let Some(cwd) = working_dir {
            cwd.join(p)
        } else {
            p
        }
    }

    fn analyze(&self, params: AnalyzeParams, path: PathBuf) -> CallToolResult {
        if !path.exists() {
            return CallToolResult::error(vec![Content::text(format!(
                "Error: path not found: {}",
                path.display()
            ))
            .with_priority(0.0)]);
        }

        if let Some(ref focus) = params.focus {
            self.focused_mode(
                &path,
                focus,
                params.follow_depth,
                params.max_depth,
                params.force,
            )
        } else if path.is_file() {
            self.semantic_mode(&path, params.force)
        } else {
            self.structure_mode(&path, params.max_depth, params.force)
        }
    }

    pub fn analyze_file(path: &Path) -> Option<FileAnalysis> {
        let source = std::fs::read_to_string(path).ok()?;
        let parser = Parser::new();
        parser.analyze_file(path, &source)
    }

    pub fn collect_files(dir: &Path, max_depth: u32) -> Vec<PathBuf> {
        let mut builder = WalkBuilder::new(dir);
        if max_depth > 0 {
            builder.max_depth(Some(max_depth as usize));
        }
        builder
            .build()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_some_and(|ft| ft.is_file()))
            .map(|e| e.into_path())
            .collect()
    }

    fn structure_mode(&self, dir: &Path, max_depth: u32, force: bool) -> CallToolResult {
        let files = Self::collect_files(dir, max_depth);
        let total_files = files.len();

        let analyses: Vec<FileAnalysis> = files
            .par_iter()
            .filter_map(|f| Self::analyze_file(f))
            .collect();

        let output = format::format_structure(&analyses, dir, max_depth, total_files);
        Self::finish(output, force)
    }

    fn semantic_mode(&self, path: &Path, force: bool) -> CallToolResult {
        match Self::analyze_file(path) {
            Some(analysis) => {
                let root = path.parent().unwrap_or(path);
                let output = format::format_semantic(&analysis, root);
                Self::finish(output, force)
            }
            None => CallToolResult::error(vec![Content::text(format!(
                "Error: could not analyze {} (unsupported language or binary file)",
                path.display()
            ))
            .with_priority(0.0)]),
        }
    }

    fn focused_mode(
        &self,
        path: &Path,
        symbol: &str,
        follow_depth: u32,
        max_depth: u32,
        force: bool,
    ) -> CallToolResult {
        let files = if path.is_file() {
            vec![path.to_path_buf()]
        } else {
            Self::collect_files(path, max_depth)
        };

        let analyses: Vec<FileAnalysis> = files
            .par_iter()
            .filter_map(|f| Self::analyze_file(f))
            .collect();

        let root = if path.is_file() {
            path.parent().unwrap_or(path)
        } else {
            path
        };
        let g = graph::CallGraph::build(&analyses);
        let output = format::format_focused(symbol, &g, follow_depth, analyses.len(), root);
        Self::finish(output, force)
    }

    fn finish(output: String, force: bool) -> CallToolResult {
        match format::check_size(&output, force) {
            Ok(text) => CallToolResult::success(vec![Content::text(text).with_priority(0.0)]),
            Err(warning) => CallToolResult::error(vec![Content::text(warning).with_priority(0.0)]),
        }
    }
}

#[async_trait]
impl McpClientTrait for AnalyzeClient {
    async fn list_tools(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        let tool = Tool::new(
            "analyze".to_string(),
            "Analyze code structure in 3 modes: 1) Directory overview - file tree with LOC/function/class counts to max_depth. 2) File details - functions, classes, imports. 3) Symbol focus - call graphs across directory to max_depth (requires file or directory path, case-sensitive). Typical flow: directory → files → symbols. Functions called >3x show •N.".to_string(),
            Self::schema::<AnalyzeParams>(),
        )
        .annotate(ToolAnnotations::from_raw(
            Some("Analyze".to_string()),
            Some(true),
            Some(false),
            Some(true),
            Some(false),
        ));

        Ok(ListToolsResult {
            tools: vec![tool],
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        ctx: &ToolCallContext,
        name: &str,
        arguments: Option<JsonObject>,
        _cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        let working_dir = ctx.working_dir.as_deref();
        match name {
            "analyze" => match Self::parse_args::<AnalyzeParams>(arguments) {
                Ok(params) => {
                    let path = Self::resolve_path(&params.path, working_dir);
                    Ok(self.analyze(params, path))
                }
                Err(error) => Ok(CallToolResult::error(vec![Content::text(format!(
                    "Error: {error}"
                ))
                .with_priority(0.0)])),
            },
            _ => Ok(CallToolResult::error(vec![Content::text(format!(
                "Error: Unknown tool: {name}"
            ))
            .with_priority(0.0)])),
        }
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        Some(&self.info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::SessionManager;
    use rmcp::model::RawContent;
    use std::fs;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn ctx() -> PlatformExtensionContext {
        PlatformExtensionContext {
            extension_manager: None,
            session_manager: Arc::new(SessionManager::new(std::env::temp_dir())),
            session: None,
            use_login_shell_path: false,
        }
    }

    fn text(result: &CallToolResult) -> &str {
        match &result.content[0].raw {
            RawContent::Text(t) => &t.text,
            _ => panic!("expected text"),
        }
    }

    #[tokio::test]
    async fn structure_mode() {
        let tmp = tempdir().unwrap();
        fs::write(
            tmp.path().join("lib.rs"),
            "use std::io;\nfn read() {}\nfn write() {}\nstruct Buffer;\n",
        )
        .unwrap();
        fs::write(
            tmp.path().join("app.py"),
            "import os\nclass App:\n    pass\ndef main():\n    pass\ndef run():\n    pass\n",
        )
        .unwrap();

        let client = AnalyzeClient::new(ctx()).unwrap();
        let result = client.analyze(
            AnalyzeParams {
                path: tmp.path().to_str().unwrap().into(),
                focus: None,
                max_depth: 3,
                follow_depth: 2,
                force: false,
            },
            tmp.path().to_path_buf(),
        );
        let out = text(&result);

        assert!(out.contains("2 files"));
        assert!(out.contains("F"));
        assert!(out.contains("lib.rs"));
        assert!(out.contains("app.py"));
        assert!(out.contains("rust"));
        assert!(out.contains("python"));
    }

    #[tokio::test]
    async fn semantic_mode() {
        let tmp = tempdir().unwrap();
        let file = tmp.path().join("demo.rs");
        fs::write(
            &file,
            r#"
use std::collections::HashMap;
use std::io;

struct Config;

fn validate(x: i32) -> bool { x > 0 }
fn process() {
    validate(1);
    validate(2);
    validate(3);
    validate(4);
    helper();
}
fn helper() { validate(0); }
"#,
        )
        .unwrap();

        let client = AnalyzeClient::new(ctx()).unwrap();
        let result = client.analyze(
            AnalyzeParams {
                path: file.to_str().unwrap().into(),
                focus: None,
                max_depth: 3,
                follow_depth: 2,
                force: false,
            },
            file.clone(),
        );
        let out = text(&result);

        // Functions listed with signatures and line numbers
        assert!(out.contains("F:"));
        assert!(out.contains("validate("));
        assert!(out.contains("process:"));
        assert!(out.contains("helper"));
        // Struct
        assert!(out.contains("C:"));
        assert!(out.contains("Config:"));
        // Imports
        assert!(out.contains("I:"));
        assert!(out.contains("std::collections::HashMap"));
        // validate called 5 times (>3) → •5
        assert!(out.contains("validate(") && out.contains("•5"));
    }

    #[tokio::test]
    async fn focused_mode() {
        let tmp = tempdir().unwrap();
        fs::write(tmp.path().join("a.rs"), "fn process() { validate(1); }\n").unwrap();
        fs::write(tmp.path().join("b.rs"), "fn validate() { process(); }\n").unwrap();

        let client = AnalyzeClient::new(ctx()).unwrap();
        let result = client.analyze(
            AnalyzeParams {
                path: tmp.path().to_str().unwrap().into(),
                focus: Some("process".into()),
                max_depth: 3,
                follow_depth: 2,
                force: false,
            },
            tmp.path().to_path_buf(),
        );
        let out = text(&result);

        assert!(out.contains("FOCUS: process"));
        assert!(out.contains("DEF"));
        assert!(out.contains("IN") || out.contains("OUT"));
        assert!(out.contains("files analyzed"));
    }

    #[tokio::test]
    async fn error_and_edge() {
        let client = AnalyzeClient::new(ctx()).unwrap();

        // Nonexistent path
        let result = client.analyze(
            AnalyzeParams {
                path: "/no/such/path".into(),
                focus: None,
                max_depth: 3,
                follow_depth: 2,
                force: false,
            },
            PathBuf::from("/no/such/path"),
        );
        assert_eq!(result.is_error, Some(true));
        assert!(text(&result).contains("path not found"));

        // Empty directory → 0 files
        let tmp = tempdir().unwrap();
        let result = client.analyze(
            AnalyzeParams {
                path: tmp.path().to_str().unwrap().into(),
                focus: None,
                max_depth: 3,
                follow_depth: 2,
                force: false,
            },
            tmp.path().to_path_buf(),
        );
        assert!(text(&result).contains("0 files"));

        // Size guard
        let big = "x".repeat(60_000);
        assert!(format::check_size(&big, false).is_err());
        assert!(format::check_size(&big, true).is_ok());
    }
}
