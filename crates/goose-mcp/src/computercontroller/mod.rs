#[cfg(not(windows))]
use crate::subprocess::merged_path;
use crate::subprocess::SubprocessExt;
#[cfg(target_os = "macos")]
use base64::Engine;
use etcetera::{choose_app_strategy, AppStrategy};
use indoc::{formatdoc, indoc};
use reqwest::{Client, Url};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        AnnotateAble, CallToolResult, Content, ErrorCode, ErrorData, Implementation,
        InitializeResult, ListResourcesResult, PaginatedRequestParams, RawResource,
        ReadResourceRequestParams, ReadResourceResult, Resource, ResourceContents,
        ServerCapabilities, ServerInfo,
    },
    schemars::JsonSchema,
    service::RequestContext,
    tool, tool_handler, tool_router, RoleServer, ServerHandler,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::PathBuf, sync::Arc, sync::Mutex};
use tokio::process::Command;

#[cfg(target_os = "macos")]
use rmcp::model::Role;
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

mod docx_tool;
mod pdf_tool;
mod xlsx_tool;

mod platform;
use platform::{create_system_automation, SystemAutomation};

/// Enum for save_as parameter in web_scrape tool
#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone, Default)]
#[serde(rename_all = "lowercase")]
pub enum SaveAsFormat {
    /// Save as text (for HTML pages)
    #[default]
    Text,
    /// Save as JSON (for API responses)
    Json,
    /// Save as binary (for images and other files)
    Binary,
}

/// Parameters for the web_scrape tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct WebScrapeParams {
    /// The URL to fetch content from
    pub url: String,
    /// Format of the response.
    #[serde(default)]
    pub save_as: SaveAsFormat,
}

/// Enum for language parameter in automation_script tool
#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "lowercase")]
pub enum ScriptLanguage {
    /// Shell/Bash script
    Shell,
    /// Batch script (Windows)
    Batch,
    /// Ruby script
    Ruby,
    /// PowerShell script
    Powershell,
}

/// Enum for command parameter in cache tool
#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "lowercase")]
pub enum CacheCommand {
    /// List all cached files
    List,
    /// View content of a cached file
    View,
    /// Delete a cached file
    Delete,
    /// Clear all cached files
    Clear,
}

/// Parameters for the automation_script tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct AutomationScriptParams {
    /// The scripting language to use
    #[serde(rename = "language")]
    pub language: ScriptLanguage,
    /// The script content
    pub script: String,
    /// Whether to save the script output to a file
    #[serde(default)]
    pub save_output: bool,
}

/// Parameters for the computer_control tool (Windows, Linux)
#[cfg(not(target_os = "macos"))]
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ComputerControlParams {
    /// The automation script content (PowerShell for Windows, shell for Linux)
    pub script: String,
    /// Whether to save the script output to a file
    #[serde(default)]
    pub save_output: bool,
}

/// Parameters for the computer_control tool (macOS — Peekaboo CLI passthrough)
#[cfg(target_os = "macos")]
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ComputerControlParams {
    /// The peekaboo subcommand and arguments as a single string.
    /// Examples:
    ///   "see --app Safari --annotate"
    ///   "click --on B1"
    ///   "type --text \"hello\" --return"
    ///   "hotkey --keys cmd,c"
    ///   "app launch Safari --open https://example.com"
    ///   "window list --app Safari --json"
    ///   "press tab --count 3"
    ///   "clipboard --action get"
    pub command: String,
    /// Whether to capture and return a screenshot as part of the result.
    /// Useful after click/type actions to see the updated UI state.
    #[serde(default)]
    pub capture_screenshot: bool,
}

/// Parameters for the cache tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CacheParams {
    /// The command to perform
    pub command: CacheCommand,
    /// Path to the cached file for view/delete commands
    pub path: Option<String>,
}

/// Parameters for the pdf_tool
/// Enum for operation parameter in pdf_tool
#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "snake_case")]
pub enum PdfOperation {
    /// Extract all text content from the PDF
    ExtractText,
    /// Extract and save embedded images to PNG files
    ExtractImages,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PdfToolParams {
    /// Path to the PDF file
    pub path: String,
    /// Operation to perform on the PDF
    pub operation: PdfOperation,
}

/// Enum for operation parameter in docx_tool
#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "snake_case")]
pub enum DocxOperation {
    /// Extract all text content and structure from the DOCX
    ExtractText,
    /// Create a new DOCX or update existing one with provided content
    UpdateDoc,
}

/// Enum for update mode in docx_tool params
#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone, Default)]
#[serde(rename_all = "snake_case")]
pub enum DocxUpdateMode {
    /// Add content to end of document (default)
    #[default]
    Append,
    /// Replace specific text with new content
    Replace,
    /// Add content with specific heading level and styling
    Structured,
    /// Add an image to the document (with optional caption)
    AddImage,
}

/// Enum for text alignment in docx_tool params
#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "lowercase")]
pub enum TextAlignment {
    /// Left alignment
    Left,
    /// Center alignment
    Center,
    /// Right alignment
    Right,
    /// Justified alignment
    Justified,
}

/// Styling options for text in docx_tool
#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone, Default)]
pub struct DocxTextStyle {
    /// Make text bold
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    /// Make text italic
    #[serde(skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    /// Make text underlined
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    /// Font size in points
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u32>,
    /// Text color in hex format (e.g., 'FF0000' for red)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Text alignment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alignment: Option<TextAlignment>,
}

/// Additional parameters for update_doc operation
#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone, Default)]
pub struct DocxUpdateParams {
    /// Update mode (default: append)
    #[serde(default)]
    pub mode: DocxUpdateMode,
    /// Text to replace (required for replace mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_text: Option<String>,
    /// Heading level for structured mode (e.g., 'Heading1', 'Heading2')
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    /// Path to the image file (required for add_image mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_path: Option<String>,
    /// Image width in pixels (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    /// Image height in pixels (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// Styling options for the text
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<DocxTextStyle>,
}

/// Parameters for the docx_tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DocxToolParams {
    /// Path to the DOCX file
    pub path: String,
    /// Operation to perform on the DOCX
    pub operation: DocxOperation,
    /// Content to write (required for update_doc operation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Additional parameters for update_doc operation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<DocxUpdateParams>,
}

/// Parameters for the xlsx_tool
/// Enum for operation parameter in xlsx_tool
#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "snake_case")]
pub enum XlsxOperation {
    /// List all worksheets in the workbook
    ListWorksheets,
    /// Get column names from a worksheet
    GetColumns,
    /// Get values and formulas from a cell range
    GetRange,
    /// Search for text in a worksheet
    FindText,
    /// Update a single cell's value
    UpdateCell,
    /// Get value and formula from a specific cell
    GetCell,
    /// Save changes back to the file
    Save,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct XlsxToolParams {
    /// Path to the XLSX file
    pub path: String,
    /// Operation to perform on the XLSX file
    pub operation: XlsxOperation,
    /// Worksheet name (if not provided, uses first worksheet)
    pub worksheet: Option<String>,
    /// Cell range in A1 notation (e.g., 'A1:C10') for get_range operation
    pub range: Option<String>,
    /// Text to search for in find_text operation
    pub search_text: Option<String>,
    /// Whether search should be case-sensitive
    #[serde(default)]
    pub case_sensitive: bool,
    /// Row number for update_cell and get_cell operations
    pub row: Option<u64>,
    /// Column number for update_cell and get_cell operations
    pub col: Option<u64>,
    /// New value for update_cell operation
    pub value: Option<String>,
}

/// ComputerController MCP Server using official RMCP SDK
#[derive(Clone)]
pub struct ComputerControllerServer {
    tool_router: ToolRouter<Self>,
    cache_dir: PathBuf,
    active_resources: Arc<Mutex<HashMap<String, ResourceContents>>>,
    http_client: Client,
    instructions: String,
    system_automation: Arc<Box<dyn SystemAutomation + Send + Sync>>,
    #[cfg(target_os = "macos")]
    peekaboo_installed: Arc<AtomicBool>,
}

impl Default for ComputerControllerServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router(router = tool_router)]
impl ComputerControllerServer {
    pub fn new() -> Self {
        // choose_app_strategy().cache_dir()
        // - macOS/Linux: ~/.cache/goose/computer_controller/
        // - Windows:     ~\AppData\Local\Block\goose\cache\computer_controller\
        // keep previous behavior of defaulting to /tmp/
        let cache_dir = choose_app_strategy(crate::APP_STRATEGY.clone())
            .map(|strategy| strategy.in_cache_dir("computer_controller"))
            .unwrap_or_else(|_| create_system_automation().get_temp_path());

        fs::create_dir_all(&cache_dir).unwrap_or_else(|_| {
            println!(
                "Warning: Failed to create cache directory at {:?}",
                cache_dir
            )
        });

        let system_automation: Arc<Box<dyn SystemAutomation + Send + Sync>> =
            Arc::new(create_system_automation());

        let has_display = system_automation.has_display();

        let os_specific_instructions = match (std::env::consts::OS, has_display) {
            ("windows", _) => indoc! {r#"
            Here are some extra tools:
            automation_script
              - Create and run PowerShell or Batch scripts
              - PowerShell is recommended for most tasks
              - Scripts can save their output to files
              - Windows-specific features:
                - PowerShell for system automation and UI control
                - Windows Management Instrumentation (WMI)
                - Registry access and system settings
              - Use the screenshot tool if needed to help with tasks

            computer_control
              - System automation using PowerShell
              - Consider the screenshot tool to work out what is on screen and what to do to help with the control task.
            "#},
            ("macos", _) => indoc! {r#"
            Here are some extra tools:
            automation_script
              - Create and run Shell, Ruby, or AppleScript scripts
              - Scripts can save their output to files

            computer_control — Peekaboo CLI for macOS UI automation (auto-installed via Homebrew).
              Peekaboo captures/inspects screens, targets UI elements, drives input, and manages
              apps/windows/menus. Pass a peekaboo subcommand string as the `command` parameter.
              Set `capture_screenshot: true` to capture the screen after actions (click, type, etc.).
              Commands support `--json`/`-j` for structured output. Run `peekaboo <cmd> --help` for
              full flags if needed.

              Quickstart (most reliable flow):
                1. command: "see --app Safari --annotate"    — get annotated screenshot with element IDs
                2. command: "click --on B3 --app Safari"     — click element B3
                3. command: "type \"user@example.com\" --app Safari"  — type text
                4. command: "press tab --count 1 --app Safari"       — press tab
                5. command: "type \"password\" --app Safari --return" — type and press enter

              Vision:
              - see — annotated UI map with element IDs and optional AI analysis
                `see --app Safari --annotate`, `see --mode screen --screen-index 0`
                `see --app Notes --analyze "describe what's on screen"`
              - image — capture screenshots without annotation
                `image --mode frontmost`, `image --mode screen --screen-index 1 --retina`
                `image --app Safari --window-title "Dashboard" --analyze "Summarize KPIs"`
              - capture — live motion-aware capture
                `capture live --mode region --region 100,100,800,600 --duration 30`

              Interaction:
              - click — by element ID, query, or coordinates with smart waits
                `click --on B1`, `click --coords 100,200`, `click --on B1 --double`, `click --on B1 --right`
              - type — text input with optional control keys
                `type "hello" --return`, `type "text" --clear --app Notes`, `type "slow" --wpm 80`
              - press — special key sequences with repeats
                `press tab --count 3`, `press escape`, `press return`, `press space`
              - hotkey — modifier key combos (comma-separated)
                `hotkey --keys cmd,c`, `hotkey --keys cmd,shift,t`, `hotkey --keys cmd,a`
              - paste — set clipboard then paste (more reliable than type for long text)
                `paste --text "long multi-line content"`
              - scroll — directional scrolling with optional targeting
                `scroll --direction down --amount 5 --smooth`, `scroll --direction up --amount 3`
              - drag — drag between elements or coordinates
                `drag --from B1 --to T2`, `drag --from-coords 100,100 --to-coords 500,300`
              - swipe — gesture-style drags
                `swipe --from-coords 100,500 --to-coords 100,200 --duration 800`
              - move — cursor positioning
                `move 500,300 --smooth`

              Apps & Windows:
              - app — launch, quit, switch, list applications
                `app launch Safari --open https://example.com`, `app quit Safari`
                `app switch Safari`, `app list`, `app hide Safari`, `app unhide Safari`
              - window — manage window position, size, focus, list
                `window list --app Safari --json`, `window focus --app Safari`
                `window set-bounds --app Safari --x 50 --y 50 --width 1200 --height 800`
                `window close --app Safari`, `window minimize --app Safari`
              - list — enumerate apps, windows, screens
                `list apps --json`, `list windows --json`, `list screens --json`
              - space — macOS Spaces (virtual desktops)
                `space list`, `space switch --index 2`

              Menus & System:
              - menu — click application menu items
                `menu click --app Safari --item "New Window"`
                `menu click --app TextEdit --path "Format > Font > Show Fonts"`
              - menubar — status bar / menu extras
                `menubar list --json`, `menubar click --title "WiFi"`
              - dock — Dock items
                `dock launch Safari`, `dock list --json`
              - dialog — system dialogs and alerts
                `dialog click --button "OK"`, `dialog list`
              - clipboard — read/write clipboard
                `clipboard --action get`, `clipboard --action set --text "content"`
              - open — open URLs or files with app targeting
                `open https://example.com --app Safari`
              - permissions — check Screen Recording / Accessibility status
                `permissions status`

              Common targeting parameters (work across most commands):
              - App/window: `--app Name`, `--pid 1234`, `--window-title "title"`, `--window-id 5678`, `--window-index 0`
              - Elements: `--on B1` (element ID from see), `--coords 100,200`
              - Snapshot reuse: `--snapshot <id>` (reuse a previous see result without re-capturing)
              - Focus: `--no-auto-focus`, `--space-switch`, `--bring-to-current-space`

              Tips:
              - Always `see --annotate` first to identify element IDs before clicking
              - Use `--json` for structured output on list/query commands
              - Use `paste` over `type` for long or multi-line text
              - Use `--screen-index` for multi-monitor setups
              - If something fails, check `permissions status` for missing permissions
              - Use `capture_screenshot: true` on click/type/press actions to verify the result
            "#},
            (_, true) => indoc! {r#"
            Here are some extra tools:
            automation_script
              - Create and run Shell scripts
              - Shell (bash) is recommended for most tasks
              - Scripts can save their output to files
              - Linux-specific features:
                - System automation through shell scripting
                - X11/Wayland window management
                - D-Bus system services integration
                - Desktop environment control
              - Use the screenshot tool if needed to help with tasks

            computer_control
              - System automation using shell commands and system tools
              - Desktop environment automation (GNOME, KDE, etc.)
              - Consider the screenshot tool to work out what is on screen and what to do to help with the control task.

            When you need to interact with websites or web applications, consider using tools like xdotool or wmctrl for:
              - Window management
              - Simulating keyboard/mouse input
              - Automating UI interactions
              - Desktop environment control
            "#},
            (_, false) => indoc! {r#"
            Here are some extra tools:
            automation_script
              - Create and run Shell scripts
              - Shell (bash) is recommended for most tasks
              - Scripts can save their output to files
              - Linux-specific features:
                - System automation through shell scripting
                - D-Bus system services integration

            Note: No display server detected (headless mode). The computer_control tool
            is not available in this environment. Use automation_script for shell-based tasks.
            "#},
        };

        let instructions = formatdoc! {r#"
            You are a helpful assistant to a power user who is not a professional developer, but you may use development tools to help assist them.
            The user may not know how to break down tasks, so you will need to ensure that you do, and run things in batches as needed.
            The ComputerControllerExtension helps you with common tasks like web scraping,
            data processing, and automation without requiring programming expertise.

            You can use scripting as needed to work with text files of data, such as csvs, json, or text files etc.
            Using the developer extension is allowed for more sophisticated tasks or instructed to (js or py can be helpful for more complex tasks if tools are available).

            Accessing web sites, even apis, may be common (you can use scripting to do this) without troubling them too much (they won't know what limits are).
            Try to do your best to find ways to complete a task without too many questions or offering options unless it is really unclear, find a way if you can.
            You can also guide them steps if they can help out as you go along.

            There is already a screenshot tool available you can use if needed to see what is on screen.

            {os_instructions}

            web_scrape
              - Fetch content from html websites and APIs
              - Save as text, JSON, or binary files
              - Content is cached locally for later use
              - This is not optimised for complex websites, so don't use this as the first tool.
            cache
              - Manage your cached files
              - List, view, delete files
              - Clear all cached data
            The extension automatically manages:
            - Cache directory: {cache_dir}
            - File organization and cleanup
            "#,
            os_instructions = os_specific_instructions,
            cache_dir = cache_dir.display()
        };

        let mut tool_router = Self::tool_router();
        if !has_display {
            tool_router.remove_route("computer_control");
        }

        Self {
            tool_router,
            cache_dir,
            active_resources: Arc::new(Mutex::new(HashMap::new())),
            http_client: Client::builder().user_agent("goose/1.0").build().unwrap(),
            instructions,
            system_automation,
            #[cfg(target_os = "macos")]
            peekaboo_installed: Arc::new(AtomicBool::new(crate::peekaboo::is_peekaboo_installed())),
        }
    }

    // Helper function to generate a cache file path
    fn get_cache_path(&self, prefix: &str, extension: &str) -> PathBuf {
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        self.cache_dir
            .join(format!("{}_{}.{}", prefix, timestamp, extension))
    }

    // Helper function to save content to cache
    async fn save_to_cache(
        &self,
        content: &[u8],
        prefix: &str,
        extension: &str,
    ) -> Result<PathBuf, ErrorData> {
        let cache_path = self.get_cache_path(prefix, extension);
        fs::write(&cache_path, content).map_err(|e| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to write to cache: {}", e),
                None,
            )
        })?;
        Ok(cache_path)
    }

    // Helper function to register a file as a resource
    fn register_as_resource(&self, cache_path: &PathBuf, mime_type: &str) -> Result<(), ErrorData> {
        let uri = Url::from_file_path(cache_path)
            .map_err(|_| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    "Invalid cache path".to_string(),
                    None,
                )
            })?
            .to_string();

        let resource = ResourceContents::TextResourceContents {
            uri: uri.clone(),
            text: String::new(), // We'll read it when needed
            mime_type: Some(mime_type.to_string()),
            meta: None,
        };

        self.active_resources.lock().unwrap().insert(uri, resource);
        Ok(())
    }

    /// Fetch and save content from a web page
    #[tool(
        name = "web_scrape",
        description = "
            Fetch and save content from a web page. The content can be saved as:
            - text (for HTML pages)
            - json (for API responses)
            - binary (for images and other files)
            Returns 'Content saved to: <path>'. Use cache to read the content.
        "
    )]
    pub async fn web_scrape(
        &self,
        params: Parameters<WebScrapeParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let url = &params.url;
        let save_as = params.save_as;

        // Fetch the content
        let response = self
            .http_client
            .get(url)
            .header("Accept", "text/markdown, */*")
            .send()
            .await
            .map_err(|e| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Failed to fetch URL: {}", e),
                    None,
                )
            })?;

        let status = response.status();
        if !status.is_success() {
            return Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("HTTP request failed with status: {}", status),
                None,
            ));
        }

        // Process based on save_as parameter
        let (content, extension, mime_type) = match save_as {
            SaveAsFormat::Text => {
                let text = response.text().await.map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to get text: {}", e),
                        None,
                    )
                })?;
                (text.into_bytes(), "txt", "text/plain")
            }
            SaveAsFormat::Json => {
                let text = response.text().await.map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to get text: {}", e),
                        None,
                    )
                })?;
                // Verify it's valid JSON
                serde_json::from_str::<serde_json::Value>(&text).map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Invalid JSON response: {}", e),
                        None,
                    )
                })?;
                (text.into_bytes(), "json", "application/json")
            }
            SaveAsFormat::Binary => {
                let bytes = response.bytes().await.map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to get bytes: {}", e),
                        None,
                    )
                })?;
                (bytes.to_vec(), "bin", "application/octet-stream")
            }
        };

        // Save to cache
        let cache_path = self.save_to_cache(&content, "web", extension).await?;

        // Register as a resource
        self.register_as_resource(&cache_path, mime_type)?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Content saved to: {}",
            cache_path.display()
        ))]))
    }

    /// Create and run small scripts for automation tasks
    #[cfg(target_os = "windows")]
    #[tool(
        name = "automation_script",
        description = "
            Create and run small PowerShell or Batch scripts for automation tasks.
            PowerShell is recommended for most tasks.

            The script is saved to a temporary file and executed.
            Some examples:
            - Sort unique lines: Get-Content file.txt | Sort-Object -Unique
            - Extract CSV column: Import-Csv file.csv | Select-Object -ExpandProperty Column2
            - Find text: Select-String -Pattern 'pattern' -Path file.txt
        "
    )]
    pub async fn automation_script(
        &self,
        params: Parameters<AutomationScriptParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.automation_script_impl(params).await
    }

    /// Create and run small scripts for automation tasks
    #[cfg(target_os = "macos")]
    #[tool(
        name = "automation_script",
        description = "
            Create and run Shell, Ruby, or AppleScript (via osascript) scripts.
            Use shell (bash) for most tasks. AppleScript for app scripting and system settings.
            Examples:
                - sort file.txt | uniq
                - awk -F ',' '{ print $2}' file.csv
                - osascript -e 'tell app \"Finder\" to get name of every window'
        "
    )]
    pub async fn automation_script(
        &self,
        params: Parameters<AutomationScriptParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.automation_script_impl(params).await
    }

    /// Create and run small scripts for automation tasks
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    #[tool(
        name = "automation_script",
        description = "
            Create and run Shell scripts for automation tasks.
            Consider using shell script (bash) for most simple tasks first.
            Examples:
                - sort file.txt | uniq
                - awk -F ',' '{ print $2}' file.csv
                - grep pattern file.txt
        "
    )]
    pub async fn automation_script(
        &self,
        params: Parameters<AutomationScriptParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.automation_script_impl(params).await
    }

    #[allow(clippy::too_many_lines)]
    async fn automation_script_impl(
        &self,
        params: Parameters<AutomationScriptParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let language = params.language;
        let script = &params.script;
        let save_output = params.save_output;

        // Create a temporary directory for the script
        let script_dir = tempfile::tempdir().map_err(|e| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to create temporary directory: {}", e),
                None,
            )
        })?;

        let (shell, shell_arg) = self.system_automation.get_shell_command();

        let command = match language {
            ScriptLanguage::Shell | ScriptLanguage::Batch => {
                let script_path = script_dir.path().join(format!(
                    "script.{}",
                    if cfg!(windows) { "bat" } else { "sh" }
                ));
                fs::write(&script_path, script).map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to write script: {}", e),
                        None,
                    )
                })?;

                // Set execute permissions on Unix systems
                #[cfg(unix)]
                {
                    let mut perms = fs::metadata(&script_path)
                        .map_err(|e| {
                            ErrorData::new(
                                ErrorCode::INTERNAL_ERROR,
                                format!("Failed to get file metadata: {}", e),
                                None,
                            )
                        })?
                        .permissions();
                    perms.set_mode(0o755); // rwxr-xr-x
                    fs::set_permissions(&script_path, perms).map_err(|e| {
                        ErrorData::new(
                            ErrorCode::INTERNAL_ERROR,
                            format!("Failed to set execute permissions: {}", e),
                            None,
                        )
                    })?;
                }

                script_path.display().to_string()
            }
            ScriptLanguage::Ruby => {
                let script_path = script_dir.path().join("script.rb");
                fs::write(&script_path, script).map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to write script: {}", e),
                        None,
                    )
                })?;

                format!("ruby {}", script_path.display())
            }
            ScriptLanguage::Powershell => {
                let script_path = script_dir.path().join("script.ps1");
                fs::write(&script_path, script).map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to write script: {}", e),
                        None,
                    )
                })?;

                script_path.display().to_string()
            }
        };

        // Run the script
        let output = match language {
            ScriptLanguage::Powershell => {
                // For PowerShell, we need to use -File instead of -Command
                Command::new("powershell")
                    .arg("-NoProfile")
                    .arg("-NonInteractive")
                    .arg("-File")
                    .arg(&command)
                    .env("GOOSE_TERMINAL", "1")
                    .env("AGENT", "goose")
                    .set_no_window()
                    .output()
                    .await
                    .map_err(|e| {
                        ErrorData::new(
                            ErrorCode::INTERNAL_ERROR,
                            format!("Failed to run script: {}", e),
                            None,
                        )
                    })?
            }
            _ => {
                let mut cmd = Command::new(shell);
                cmd.arg(shell_arg)
                    .arg(&command)
                    .env("GOOSE_TERMINAL", "1")
                    .env("AGENT", "goose");
                #[cfg(not(windows))]
                if let Some(path) = merged_path() {
                    cmd.env("PATH", path);
                }
                cmd.set_no_window().output().await.map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to run script: {}", e),
                        None,
                    )
                })?
            }
        };

        let output_str = String::from_utf8_lossy(&output.stdout).into_owned();
        let error_str = String::from_utf8_lossy(&output.stderr).into_owned();

        let mut result = if output.status.success() {
            format!("Script completed successfully.\n\nOutput:\n{}", output_str)
        } else {
            format!(
                "Script failed with error code {}.\n\nError:\n{}\nOutput:\n{}",
                output.status, error_str, output_str
            )
        };

        // Save output if requested
        if save_output && !output_str.is_empty() {
            let cache_path = self
                .save_to_cache(output_str.as_bytes(), "script_output", "txt")
                .await?;
            result.push_str(&format!("\n\nOutput saved to: {}", cache_path.display()));

            // Register as a resource
            self.register_as_resource(&cache_path, "text")?;
        }

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Control the computer using system automation
    #[cfg(target_os = "windows")]
    #[tool(
        name = "computer_control",
        description = "
            Control the computer using Windows system automation.

            Features available:
            - PowerShell automation for system control
            - UI automation through PowerShell
            - File and system management
            - Windows-specific features and settings

            Can be combined with screenshot tool for visual task assistance.
        "
    )]
    pub async fn computer_control(
        &self,
        params: Parameters<ComputerControlParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.computer_control_impl(params).await
    }

    /// Control the computer using Peekaboo CLI for macOS GUI automation.
    /// Auto-installs via Homebrew on first use.
    #[cfg(target_os = "macos")]
    #[tool(
        name = "computer_control",
        description = "
            macOS UI automation via Peekaboo CLI. Pass a subcommand string as `command`.

            Core workflow: see → click → type
            1. see --app Safari --annotate  (get annotated screenshot with element IDs)
            2. click --on B3               (click element by ID)
            3. type \"hello\" --return       (type text, press enter)

            Key commands: see, image, click, type, press, hotkey, paste, scroll, drag,
            swipe, move, app, window, list, menu, menubar, dock, dialog, clipboard,
            space, open, permissions.

            Targeting: --app Name, --window-title, --window-id, --on ID, --coords x,y
            Set capture_screenshot=true to verify UI state after actions.
            See extension instructions for full command reference and examples.
        "
    )]
    pub async fn computer_control(
        &self,
        params: Parameters<ComputerControlParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.peekaboo_impl(params).await
    }

    /// Control the computer using system automation
    #[cfg(target_os = "linux")]
    #[tool(
        name = "computer_control",
        description = "
            Control the computer using Linux system automation.

            Features available:
            - Shell scripting for system control
            - X11/Wayland window management
            - D-Bus for system services
            - File and system management
            - Desktop environment control (GNOME, KDE, etc.)
            - Process management and monitoring
            - System settings and configurations

            Can be combined with screenshot tool for visual task assistance.
        "
    )]
    pub async fn computer_control(
        &self,
        params: Parameters<ComputerControlParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.computer_control_impl(params).await
    }

    /// Control the computer using system automation (fallback for other OS)
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    #[tool(
        name = "computer_control",
        description = "Control the computer using system automation. Features available depend on your operating system. Can be combined with screenshot tool for visual task assistance."
    )]
    pub async fn computer_control(
        &self,
        params: Parameters<ComputerControlParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.computer_control_impl(params).await
    }

    #[cfg(not(target_os = "macos"))]
    async fn computer_control_impl(
        &self,
        params: Parameters<ComputerControlParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let script = &params.script;
        let save_output = params.save_output;

        // Use platform-specific automation
        let output = self
            .system_automation
            .execute_system_script(script)
            .map_err(|e| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Failed to execute script: {}", e),
                    None,
                )
            })?;

        let mut result = format!("Script completed successfully.\n\nOutput:\n{}", output);

        // Save output if requested
        if save_output && !output.is_empty() {
            let cache_path = self
                .save_to_cache(output.as_bytes(), "automation_output", "txt")
                .await?;
            result.push_str(&format!("\n\nOutput saved to: {}", cache_path.display()));

            // Register as a resource
            self.register_as_resource(&cache_path, "text")?;
        }

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[cfg(target_os = "macos")]
    fn ensure_peekaboo(&self) -> Result<(), ErrorData> {
        if self.peekaboo_installed.load(Ordering::Relaxed) {
            return Ok(());
        }
        if crate::peekaboo::is_peekaboo_installed() {
            self.peekaboo_installed.store(true, Ordering::Relaxed);
            return Ok(());
        }
        tracing::info!("Peekaboo not found, attempting auto-install via brew");
        match crate::peekaboo::auto_install_peekaboo() {
            Ok(()) => {
                self.peekaboo_installed.store(true, Ordering::Relaxed);
                tracing::info!("Peekaboo installed successfully");
                Ok(())
            }
            Err(msg) => Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!(
                    "Peekaboo is not installed and auto-install failed: {}\n\
                     Install manually with: brew install steipete/tap/peekaboo\n\
                     Peekaboo requires macOS 15+ (Sequoia) with Screen Recording and Accessibility permissions.",
                    msg
                ),
                None,
            )),
        }
    }

    #[cfg(target_os = "macos")]
    fn run_peekaboo_cmd(&self, args: &[&str]) -> Result<String, ErrorData> {
        let mut cmd = std::process::Command::new("peekaboo");
        cmd.args(args);
        if let Some(path) = merged_path() {
            cmd.env("PATH", path);
        }
        let output = cmd.output().map_err(|e| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to run peekaboo: {}", e),
                None,
            )
        })?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if !output.status.success() {
            return Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!(
                    "peekaboo {} failed (exit {}):\n{}\n{}",
                    args.first().unwrap_or(&""),
                    output.status,
                    stderr.trim(),
                    stdout.trim()
                ),
                None,
            ));
        }
        Ok(stdout)
    }

    #[cfg(target_os = "macos")]
    async fn peekaboo_impl(
        &self,
        params: Parameters<ComputerControlParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.ensure_peekaboo()?;
        let params = params.0;

        let args = shell_words::split(&params.command).map_err(|e| {
            ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                format!("Failed to parse command: {}", e),
                None,
            )
        })?;
        if args.is_empty() {
            return Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                "Command cannot be empty".to_string(),
                None,
            ));
        }

        let is_see = args[0] == "see";
        let is_image = args[0] == "image";
        let screenshot_path = if is_see || is_image {
            Some(self.get_cache_path(&args[0], "png"))
        } else {
            None
        };

        let mut full_args: Vec<String> = args.clone();

        if let Some(ref path) = screenshot_path {
            if !full_args.iter().any(|a| a == "--path") {
                full_args.push("--path".to_string());
                full_args.push(path.to_string_lossy().to_string());
            }
        }
        if is_see && !full_args.iter().any(|a| a == "--json-output") {
            full_args.push("--json-output".to_string());
        }

        let wants_json = matches!(
            args[0].as_str(),
            "list" | "window" | "menubar" | "permissions" | "clipboard"
        );
        if wants_json
            && !full_args.iter().any(|a| a == "--json" || a == "-j")
            && !full_args.iter().any(|a| a == "--json-output")
        {
            full_args.push("--json".to_string());
        }

        let arg_refs: Vec<&str> = full_args.iter().map(|s| s.as_str()).collect();
        let stdout = self.run_peekaboo_cmd(&arg_refs)?;

        let mut contents = Vec::new();

        if let Some(ref path) = screenshot_path {
            let annotated = path.to_string_lossy().replace(".png", "_annotated.png");
            let image_path = if is_see && std::path::Path::new(&annotated).exists() {
                PathBuf::from(&annotated)
            } else {
                path.clone()
            };
            if image_path.exists() {
                if let Ok(bytes) = fs::read(&image_path) {
                    let data = base64::prelude::BASE64_STANDARD.encode(&bytes);
                    contents.push(Content::image(data, "image/png").with_priority(0.0));
                }
            }
        }

        if params.capture_screenshot && screenshot_path.is_none() {
            let cap_path = self.get_cache_path("peekaboo_capture", "png");
            let cap_path_str = cap_path.to_string_lossy().to_string();
            if self
                .run_peekaboo_cmd(&["image", "--mode", "frontmost", "--path", &cap_path_str])
                .is_ok()
                && cap_path.exists()
            {
                if let Ok(bytes) = fs::read(&cap_path) {
                    let data = base64::prelude::BASE64_STANDARD.encode(&bytes);
                    contents.push(Content::image(data, "image/png").with_priority(0.0));
                }
            }
        }

        let text = if stdout.len() > 12000 {
            let truncated: String = stdout.chars().take(12000).collect();
            format!(
                "{}\n\n[Output truncated. {} total chars.]",
                truncated,
                stdout.len()
            )
        } else {
            stdout
        };

        contents.insert(0, Content::text(&text).with_audience(vec![Role::Assistant]));

        Ok(CallToolResult::success(contents))
    }

    /// Process Excel (XLSX) files to read and manipulate spreadsheet data
    #[tool(
        name = "xlsx_tool",
        description = "
            Process Excel (XLSX) files to read and manipulate spreadsheet data.
            Supports operations:
            - list_worksheets: List all worksheets in the workbook (returns name, index, column_count, row_count)
            - get_columns: Get column names from a worksheet (returns values from the first row)
            - get_range: Get values and formulas from a cell range (e.g., 'A1:C10') (returns a 2D array organized as [row][column])
            - find_text: Search for text in a worksheet (returns a list of (row, column) coordinates)
            - update_cell: Update a single cell's value (returns confirmation message)
            - get_cell: Get value and formula from a specific cell (returns both value and formula if present)
            - save: Save changes back to the file (returns confirmation message)

            Use this when working with Excel spreadsheets to analyze or modify data.
        "
    )]
    pub async fn xlsx_tool(
        &self,
        params: Parameters<XlsxToolParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let path = &params.path;
        let operation = params.operation;

        match operation {
            XlsxOperation::ListWorksheets => {
                let xlsx = xlsx_tool::XlsxTool::new(path)
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                let worksheets = xlsx
                    .list_worksheets()
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "{:#?}",
                    worksheets
                ))]))
            }
            XlsxOperation::GetColumns => {
                let xlsx = xlsx_tool::XlsxTool::new(path)
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                let worksheet = if let Some(name) = &params.worksheet {
                    xlsx.get_worksheet_by_name(name).map_err(|e| {
                        ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
                    })?
                } else {
                    xlsx.get_worksheet_by_index(0).map_err(|e| {
                        ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
                    })?
                };
                let columns = xlsx
                    .get_column_names(worksheet)
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "{:#?}",
                    columns
                ))]))
            }
            XlsxOperation::GetRange => {
                let range = params.range.as_ref().ok_or_else(|| {
                    ErrorData::new(
                        ErrorCode::INVALID_PARAMS,
                        "Missing 'range' parameter".to_string(),
                        None,
                    )
                })?;

                let xlsx = xlsx_tool::XlsxTool::new(path)
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                let worksheet = if let Some(name) = &params.worksheet {
                    xlsx.get_worksheet_by_name(name).map_err(|e| {
                        ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
                    })?
                } else {
                    xlsx.get_worksheet_by_index(0).map_err(|e| {
                        ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
                    })?
                };
                let range_data = xlsx
                    .get_range(worksheet, range)
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "{:#?}",
                    range_data
                ))]))
            }
            XlsxOperation::FindText => {
                let search_text = params.search_text.as_ref().ok_or_else(|| {
                    ErrorData::new(
                        ErrorCode::INVALID_PARAMS,
                        "Missing 'search_text' parameter".to_string(),
                        None,
                    )
                })?;

                let case_sensitive = params.case_sensitive;

                let xlsx = xlsx_tool::XlsxTool::new(path)
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                let worksheet = if let Some(name) = &params.worksheet {
                    xlsx.get_worksheet_by_name(name).map_err(|e| {
                        ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
                    })?
                } else {
                    xlsx.get_worksheet_by_index(0).map_err(|e| {
                        ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
                    })?
                };
                let matches = xlsx
                    .find_in_worksheet(worksheet, search_text, case_sensitive)
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Found matches at: {:#?}",
                    matches
                ))]))
            }
            XlsxOperation::UpdateCell => {
                let row = params.row.ok_or_else(|| {
                    ErrorData::new(
                        ErrorCode::INVALID_PARAMS,
                        "Missing 'row' parameter".to_string(),
                        None,
                    )
                })?;
                let col = params.col.ok_or_else(|| {
                    ErrorData::new(
                        ErrorCode::INVALID_PARAMS,
                        "Missing 'col' parameter".to_string(),
                        None,
                    )
                })?;
                let value = params.value.as_ref().ok_or_else(|| {
                    ErrorData::new(
                        ErrorCode::INVALID_PARAMS,
                        "Missing 'value' parameter".to_string(),
                        None,
                    )
                })?;

                let worksheet_name = params.worksheet.as_deref().unwrap_or("Sheet1");

                let mut xlsx = xlsx_tool::XlsxTool::new(path)
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                xlsx.update_cell(worksheet_name, row as u32, col as u32, value)
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                xlsx.save(path)
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Updated cell ({}, {}) to '{}' in worksheet '{}'",
                    row, col, value, worksheet_name
                ))]))
            }
            XlsxOperation::Save => {
                let xlsx = xlsx_tool::XlsxTool::new(path)
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                xlsx.save(path)
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                Ok(CallToolResult::success(vec![Content::text(
                    "File saved successfully.",
                )]))
            }
            XlsxOperation::GetCell => {
                let row = params.row.ok_or_else(|| {
                    ErrorData::new(
                        ErrorCode::INVALID_PARAMS,
                        "Missing 'row' parameter".to_string(),
                        None,
                    )
                })?;

                let col = params.col.ok_or_else(|| {
                    ErrorData::new(
                        ErrorCode::INVALID_PARAMS,
                        "Missing 'col' parameter".to_string(),
                        None,
                    )
                })?;

                let xlsx = xlsx_tool::XlsxTool::new(path)
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                let worksheet = if let Some(name) = &params.worksheet {
                    xlsx.get_worksheet_by_name(name).map_err(|e| {
                        ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
                    })?
                } else {
                    xlsx.get_worksheet_by_index(0).map_err(|e| {
                        ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
                    })?
                };
                let cell_value = xlsx
                    .get_cell_value(worksheet, row as u32, col as u32)
                    .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "{:#?}",
                    cell_value
                ))]))
            }
        }
    }

    /// Process DOCX files to extract text and create/update documents
    #[tool(
        name = "docx_tool",
        description = "
            Process DOCX files to extract text and create/update documents.
            Supports operations:
            - extract_text: Extract all text content and structure (headings, TOC) from the DOCX
            - update_doc: Create a new DOCX or update existing one with provided content
              Modes:
              - append: Add content to end of document (default)
              - replace: Replace specific text with new content
              - structured: Add content with specific heading level and styling
              - add_image: Add an image to the document (with optional caption)

            Use this when there is a .docx file that needs to be processed or created.
        "
    )]
    pub async fn docx_tool(
        &self,
        params: Parameters<DocxToolParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let path = &params.path;
        let operation = params.operation;

        // Convert enum to string for the existing implementation
        let operation_str = match operation {
            DocxOperation::ExtractText => "extract_text",
            DocxOperation::UpdateDoc => "update_doc",
        };

        // Convert typed params back to JSON for the internal docx_tool impl
        let json_params = params
            .params
            .as_ref()
            .map(|p| serde_json::to_value(p).unwrap_or(serde_json::Value::Null));

        let result = crate::computercontroller::docx_tool::docx_tool(
            path,
            operation_str,
            params.content.as_deref(),
            json_params.as_ref(),
        )
        .await
        .map_err(|e| ErrorData::new(e.code, e.message, e.data))?;

        Ok(CallToolResult::success(result))
    }

    /// Process PDF files to extract text and images
    #[tool(
        name = "pdf_tool",
        description = "
            Process PDF files to extract text and images.
            Supports operations:
            - extract_text: Extract all text content from the PDF
            - extract_images: Extract and save embedded images to PNG files

            Use this when there is a .pdf file or files that need to be processed.
        "
    )]
    pub async fn pdf_tool(
        &self,
        params: Parameters<PdfToolParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let path = &params.path;
        let operation = params.operation;

        // Convert enum to string for the existing implementation
        let operation_str = match operation {
            PdfOperation::ExtractText => "extract_text",
            PdfOperation::ExtractImages => "extract_images",
        };

        let result =
            crate::computercontroller::pdf_tool::pdf_tool(path, operation_str, &self.cache_dir)
                .await
                .map_err(|e| ErrorData::new(e.code, e.message, e.data))?;

        Ok(CallToolResult::success(result))
    }

    /// Manage cached files and data
    #[tool(
        name = "cache",
        description = "
            Manage cached files and data:
            - list: List all cached files
            - view: View content of a cached file
            - delete: Delete a cached file
            - clear: Clear all cached files
        "
    )]
    pub async fn cache(
        &self,
        params: Parameters<CacheParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let command = params.0.command;
        let path = params.0.path.as_deref();

        match command {
            CacheCommand::List => {
                let mut files = Vec::new();
                for entry in fs::read_dir(&self.cache_dir).map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to read cache directory: {}", e),
                        None,
                    )
                })? {
                    let entry = entry.map_err(|e| {
                        ErrorData::new(
                            ErrorCode::INTERNAL_ERROR,
                            format!("Failed to read directory entry: {}", e),
                            None,
                        )
                    })?;
                    files.push(format!("{}", entry.path().display()));
                }
                files.sort();
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Cached files:\n{}",
                    files.join("\n")
                ))]))
            }
            CacheCommand::View => {
                let path = path.ok_or_else(|| {
                    ErrorData::new(
                        ErrorCode::INVALID_PARAMS,
                        "Missing 'path' parameter for view".to_string(),
                        None,
                    )
                })?;

                let content = fs::read_to_string(path).map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to read file: {}", e),
                        None,
                    )
                })?;

                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Content of {}:\n\n{}",
                    path, content
                ))]))
            }
            CacheCommand::Delete => {
                let path = path.ok_or_else(|| {
                    ErrorData::new(
                        ErrorCode::INVALID_PARAMS,
                        "Missing 'path' parameter for delete".to_string(),
                        None,
                    )
                })?;

                fs::remove_file(path).map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to delete file: {}", e),
                        None,
                    )
                })?;

                // Remove from active resources if present
                if let Ok(url) = Url::from_file_path(path) {
                    self.active_resources
                        .lock()
                        .unwrap()
                        .remove(&url.to_string());
                }

                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Deleted file: {}",
                    path
                ))]))
            }
            CacheCommand::Clear => {
                fs::remove_dir_all(&self.cache_dir).map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to clear cache directory: {}", e),
                        None,
                    )
                })?;
                fs::create_dir_all(&self.cache_dir).map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to recreate cache directory: {}", e),
                        None,
                    )
                })?;

                // Clear active resources
                self.active_resources.lock().unwrap().clear();

                Ok(CallToolResult::success(vec![Content::text(
                    "Cache cleared successfully.",
                )]))
            }
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for ComputerControllerServer {
    fn get_info(&self) -> ServerInfo {
        InitializeResult::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(Implementation::new(
            "goose-computercontroller",
            env!("CARGO_PKG_VERSION"),
        ))
        .with_instructions(self.instructions.clone())
    }

    async fn list_resources(
        &self,
        _pagination: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        let active_resources = self.active_resources.lock().unwrap();
        let resources: Vec<Resource> = active_resources
            .keys()
            .map(|uri| {
                RawResource::new(
                    uri.clone(),
                    uri.split('/').next_back().unwrap_or("").to_string(),
                )
                .no_annotation()
            })
            .collect();
        Ok(ListResourcesResult {
            resources,
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        params: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, ErrorData> {
        let active_resources = self.active_resources.lock().unwrap();
        let resource = active_resources.get(&params.uri).ok_or_else(|| {
            ErrorData::new(
                ErrorCode::INVALID_REQUEST,
                format!("Resource not found: {}", params.uri),
                None,
            )
        })?;

        // Clone the resource to return
        Ok(ReadResourceResult::new(vec![resource.clone()]))
    }
}
