pub mod kernel;

use super::developer::image::{load_image_content, CropParams};
use crate::agents::extension::PlatformExtensionContext;
use crate::agents::mcp_client::{Error, McpClientTrait};
use crate::agents::tool_execution::ToolCallContext;
use crate::config::Config;
use crate::session::Session;
use anyhow::Result;
use async_trait::async_trait;
use indoc::indoc;
use kernel::{ExecOutcome, ImageRequest, Kernel, KernelSpec, RunningKernel};
use rmcp::model::{
    CallToolResult, ContentBlock, Implementation, InitializeResult, JsonObject, ListToolsResult,
    ServerCapabilities, Tool, ToolAnnotations,
};
use schemars::{schema_for, JsonSchema};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, Once, Weak};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

pub static EXTENSION_NAME: &str = "python_session";
const PYTHON_TOOL_NAME: &str = "python";
const MAX_IMAGES_PER_CELL: usize = 8;
const NS_PROBE_TIMEOUT: Duration = Duration::from_millis(500);
const CHDIR_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_CELL_TIMEOUT_SECS: u64 = 120;
const IDLE_KERNEL_TTL: Duration = Duration::from_secs(30 * 60);
const REAPER_TICK: Duration = Duration::from_secs(60);

const INTERPRETER_KEY: &str = "GOOSE_PYTHON_SESSION_PYTHON";
const CELL_TIMEOUT_KEY: &str = "GOOSE_PYTHON_SESSION_CELL_TIMEOUT_SECS";
const MAX_OUTPUT_CHARS_KEY: &str = "GOOSE_PYTHON_SESSION_MAX_OUTPUT_CHARS";

const KERNEL_RESET_NOTICE: &str = "[python session was restarted: variables, imports, and \
    functions from before this point no longer exist; recreate what you need]";

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct PythonParams {
    /// Python code to run in the persistent session.
    code: String,
}

struct PythonOutput {
    text: String,
    images: Vec<ImageRequest>,
    is_error: bool,
}

struct SessionSlot {
    kernel: Option<Kernel>,
    state_path: Option<PathBuf>,
    /// `created_at` (microseconds) of the session this kernel was spawned for;
    /// a reused session id with a new creation time is a different incarnation.
    incarnation: Option<i64>,
    reset_pending: bool,
    restore_notice: Option<String>,
    last_used: Instant,
}

impl Default for SessionSlot {
    fn default() -> Self {
        Self {
            kernel: None,
            state_path: None,
            incarnation: None,
            reset_pending: false,
            restore_notice: None,
            last_used: Instant::now(),
        }
    }
}

type Sessions = Mutex<HashMap<String, Arc<tokio::sync::Mutex<SessionSlot>>>>;

pub struct PythonSessionClient {
    info: InitializeResult,
    context: PlatformExtensionContext,
    sessions: Arc<Sessions>,
    ns_cache: Mutex<HashMap<String, String>>,
    compacted: Mutex<HashSet<String>>,
    pending_dir: Mutex<HashMap<String, PathBuf>>,
    interpreter: tokio::sync::OnceCell<PathBuf>,
    resolved_path: tokio::sync::OnceCell<Option<String>>,
    reaper: Once,
}

impl PythonSessionClient {
    pub fn new(context: PlatformExtensionContext) -> Result<Self> {
        let info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new(EXTENSION_NAME.to_string(), "1.0.0".to_string())
                    .with_title("Python Session"),
            )
            .with_instructions(
                indoc! {r#"
                You have a persistent Python session: variables, imports, functions, and open
                resources persist across `python` calls for the whole conversation, and they
                survive conversation compaction. Reuse variables you have already defined instead
                of recomputing or re-reading them; after a compaction, a `<python-session>` block
                in the turn context re-lists what still exists.

                Work in three phases; do not skip the first or the last:
                1. INSPECT before you compute. Never guess an input's format. Print a real
                   sample of the actual data (a genuine line, the raw bytes, the file header,
                   the directory listing) and confirm the exact task requirements before you
                   write the solution. Most wrong answers come from computing confidently on an
                   assumed structure.
                2. SOLVE. Read, search, and transform data in Python and ASSIGN results to
                   variables so later steps are cheap and survive compaction. Large data can
                   stay in variables; print the slice you need to reason about the next step.
                3. VERIFY before you finish. Re-read what success requires and check your output
                   against it. If the task states how it will be judged (a command to run, a
                   file to produce, an expected value), run that exact check and confirm it
                   passes. Inspect the produced artifact directly - open the file you wrote,
                   diff it against the spec. Do not declare the task done on the assumption that
                   your code worked; declare it done only after you have seen it pass.

                Tools:
                - Shell commands: r = sh("pytest -q 2>&1"); then inspect r.code, r.out, r.err
                  (full output is retained on the object; the echoed form is a capped tail).
                  Use sh to run the task's own tests or acceptance commands as your verification.
                - File edits: edit(path, old, new) replaces a unique occurrence; edit(path, "",
                  content) creates a file. pathlib is also fine.
                - Images: view_image(path, crop=None) returns an image (a screenshot, diagram,
                  or photo) to you as pixels; pass a local path or http(s) URL. To read a
                  screenshot of code or a diagram, VIEW it - do not OCR it. crop=(x, y, width,
                  height) zooms into a region. You may view several by calling it in a loop.
                - The last expression in a cell is echoed like a REPL; end a cell with the
                  value you want to see.
            "#}
                .to_string(),
            );

        Ok(Self {
            info,
            context,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            ns_cache: Mutex::new(HashMap::new()),
            compacted: Mutex::new(HashSet::new()),
            pending_dir: Mutex::new(HashMap::new()),
            interpreter: tokio::sync::OnceCell::new(),
            resolved_path: tokio::sync::OnceCell::new(),
            reaper: Once::new(),
        })
    }

    fn get_tools() -> Vec<Tool> {
        let schema = schema_for!(PythonParams);
        let schema_value =
            serde_json::to_value(schema).expect("failed to serialize PythonParams schema");

        vec![Tool::new(
            PYTHON_TOOL_NAME.to_string(),
            indoc! {r#"
                Run Python in a persistent session (one per conversation). State persists
                across calls and survives compaction: variables, imports, functions, cwd.

                Inspect the real inputs before you compute, and verify your output against the
                task's success criteria (run its tests/acceptance commands) before you finish.

                Semantics:
                - The trailing expression of the cell is echoed (like a REPL). `_` holds it.
                - stdout/stderr/echo are each capped; assign large data to variables and
                  print slices instead of dumping it.
                - sh(command, timeout=None) runs a shell command and returns a result object
                  (.code/.out/.err, full output retained); edit(path, old, new) does a
                  unique-match text replacement.
                - view_image(path, crop=None) attaches an image (local path or http(s) URL) to
                  the result so you can see it; prefer this over OCR to read text in a picture.
                - Long-running cells are interrupted after a timeout with KeyboardInterrupt;
                  the session and its variables survive the interrupt.
            "#}
            .to_string(),
            schema_value.as_object().unwrap().clone(),
        )
        .annotate(ToolAnnotations::from_raw(
            Some("Run Python".to_string()),
            Some(false),
            Some(true),
            Some(false),
            Some(true),
        ))]
    }

    fn session_slot(&self, session_id: &str) -> Arc<tokio::sync::Mutex<SessionSlot>> {
        self.sessions
            .lock()
            .unwrap()
            .entry(session_id.to_string())
            .or_default()
            .clone()
    }

    fn live_slots(sessions: &Sessions) -> Vec<Arc<tokio::sync::Mutex<SessionSlot>>> {
        sessions.lock().unwrap().values().cloned().collect()
    }

    /// Kernels idle for half an hour are killed; the per-cell snapshot makes the
    /// next call a transparent restore, so long-lived goosed agents do not pin a
    /// Python process per session forever.
    fn ensure_reaper(&self) {
        self.reaper.call_once(|| {
            let sessions: Weak<Sessions> = Arc::downgrade(&self.sessions);
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(REAPER_TICK);
                loop {
                    tick.tick().await;
                    let Some(sessions) = sessions.upgrade() else {
                        return;
                    };
                    for slot in Self::live_slots(&sessions) {
                        if let Ok(mut slot) = slot.try_lock() {
                            if slot.last_used.elapsed() >= IDLE_KERNEL_TTL {
                                if let Some(mut kernel) = slot.kernel.take() {
                                    kernel.kill();
                                }
                            }
                        }
                    }
                }
            });
        });
    }

    /// The PATH the kernel and its `sh()` subprocesses should see. Desktop launches
    /// goosed with a minimal PATH, so resolve the login-shell PATH once (as the
    /// Developer shell does) and reuse it for both interpreter discovery and the
    /// kernel environment; otherwise `sh("rg ...")`, `cargo`, `pnpm` fail.
    async fn resolved_path(&self) -> Option<String> {
        let use_login_shell_path = self.context.use_login_shell_path;
        self.resolved_path
            .get_or_init(|| async move {
                tokio::task::spawn_blocking(move || {
                    login_shell_path(use_login_shell_path).or_else(|| std::env::var("PATH").ok())
                })
                .await
                .ok()
                .flatten()
            })
            .await
            .clone()
    }

    async fn child_env(&self) -> Vec<(&'static str, String)> {
        let mut env = Vec::new();
        if let Some(chars) = config_value::<u64>(MAX_OUTPUT_CHARS_KEY) {
            env.push((MAX_OUTPUT_CHARS_KEY, chars.to_string()));
        }
        if let Some(path) = self.resolved_path().await {
            env.push(("PATH", path));
        }
        env
    }

    async fn interpreter(&self) -> Result<PathBuf, String> {
        let path = self.resolved_path().await;
        self.interpreter
            .get_or_try_init(|| async move {
                tokio::task::spawn_blocking(move || discover_interpreter(path))
                    .await
                    .map_err(|e| e.to_string())?
            })
            .await
            .cloned()
    }

    async fn ensure_kernel(
        &self,
        slot: &mut SessionSlot,
        session_id: &str,
        working_dir: Option<PathBuf>,
    ) -> Result<(), String> {
        if slot.kernel.is_some() {
            return Ok(());
        }
        let session = self.load_session(session_id).await;
        let spec = KernelSpec {
            python: self.interpreter().await?,
            working_dir: working_dir
                .or_else(|| session.as_ref().map(|session| session.working_dir.clone()))
                .unwrap_or_else(|| PathBuf::from(".")),
            state_path: session
                .as_ref()
                .filter(|_| prepare_state_dir())
                .map(snapshot_path),
            env: self.child_env().await,
        };
        let kernel = Kernel::spawn(&spec).await.map_err(|e| format!("{e:#}"))?;
        slot.state_path = spec.state_path;
        slot.incarnation = session.as_ref().map(|s| s.created_at.timestamp_micros());
        let restored = kernel.restored_names();
        let dropped = kernel.dropped_names();
        if !restored.is_empty() || !dropped.is_empty() {
            let mut notice = String::from("[python session restored from a previous process");
            if !restored.is_empty() {
                notice.push_str(&format!("; available again: {}", restored.join(", ")));
            }
            if !dropped.is_empty() {
                notice.push_str(&format!(
                    "; not restored (too large or not picklable): {}",
                    dropped.join(", ")
                ));
            }
            notice.push(']');
            slot.restore_notice = Some(notice);
        }
        slot.kernel = Some(kernel);
        Ok(())
    }

    async fn load_session(&self, session_id: &str) -> Option<Session> {
        self.context
            .session_manager
            .get_session(session_id, false)
            .await
            .ok()
    }

    async fn working_dir(&self, ctx: &ToolCallContext) -> PathBuf {
        if let Some(dir) = &ctx.working_dir {
            return dir.clone();
        }
        match self.load_session(&ctx.session_id).await {
            Some(session) => session.working_dir,
            None => PathBuf::from("."),
        }
    }

    async fn run_python(
        &self,
        ctx: &ToolCallContext,
        arguments: Option<JsonObject>,
        cancellation_token: CancellationToken,
    ) -> Result<PythonOutput, String> {
        let code = arguments
            .as_ref()
            .and_then(|args| args.get("code"))
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: code")?
            .to_string();

        self.ensure_reaper();
        let slot = self.session_slot(&ctx.session_id);
        let mut slot = slot.lock().await;
        slot.last_used = Instant::now();

        self.ensure_kernel(&mut slot, &ctx.session_id, ctx.working_dir.clone())
            .await?;

        let pending_dir = self.pending_dir.lock().unwrap().remove(&ctx.session_id);
        if let Some(dir) = pending_dir {
            if let Some(kernel) = slot.kernel.as_mut() {
                if let Err(e) = kernel.chdir(&dir, CHDIR_TIMEOUT).await {
                    tracing::warn!("python session could not change directory: {e:#}");
                }
            }
        }

        let restore_notice = slot.restore_notice.take();
        let incarnation = slot.incarnation;

        // Own the kernel for the duration of the cell. If this request future is
        // dropped mid-cell (user stop, loop teardown), the guard kills the kernel
        // so the still-running cell and its subprocesses do not outlive the
        // request and block the next call; `kill_on_drop` cannot help while the
        // kernel lives in the slot.
        let mut running = RunningKernel::new(slot.kernel.take().expect("kernel was just ensured"));
        let exec_result = running
            .exec(&code, cell_timeout(), cancellation_token)
            .await;
        let kernel = running.finish();
        slot.kernel = Some(kernel);

        // The driver snapshots after every cell, so a session deleted (or its id
        // reused for a new conversation) while this cell ran has just re-created a
        // snapshot after the deletion sweep. Compare the incarnation, not mere
        // existence, so a reused id does not look alive.
        if self.session_changed(&ctx.session_id, incarnation).await {
            if let Some(mut kernel) = slot.kernel.take() {
                kernel.kill();
            }
            if let Some(path) = &slot.state_path {
                let _ = std::fs::remove_file(path);
            }
            self.ns_cache.lock().unwrap().remove(&ctx.session_id);
            return Err("the session was deleted while the cell was running".to_string());
        }

        let outcome = match exec_result {
            Ok(outcome) => outcome,
            Err(e) => {
                if let Some(mut dead) = slot.kernel.take() {
                    dead.kill();
                }
                slot.reset_pending = true;
                self.ns_cache.lock().unwrap().remove(&ctx.session_id);
                return Err(format!("{e:#}\n{KERNEL_RESET_NOTICE}"));
            }
        };

        // A snapshot restore after a crash brings the variables back, so the
        // restore notice is the accurate one and the reset notice is suppressed.
        let reset_notice = std::mem::take(&mut slot.reset_pending) && restore_notice.is_none();

        if let Ok(listing) = slot
            .kernel
            .as_mut()
            .expect("kernel survived exec")
            .namespace(NS_PROBE_TIMEOUT)
            .await
        {
            self.ns_cache
                .lock()
                .unwrap()
                .insert(ctx.session_id.clone(), listing);
        }
        slot.last_used = Instant::now();

        Ok(PythonOutput {
            text: format_outcome(&outcome, reset_notice, restore_notice),
            is_error: outcome.error.is_some(),
            images: outcome.images,
        })
    }

    async fn assemble_result(&self, ctx: &ToolCallContext, output: PythonOutput) -> CallToolResult {
        let is_error = output.is_error;
        let mut blocks = vec![ContentBlock::text(output.text)];

        if !output.images.is_empty() {
            let total = output.images.len();
            let working_dir = self.working_dir(ctx).await;
            for req in output.images.into_iter().take(MAX_IMAGES_PER_CELL) {
                let crop = req.crop.map(|c| CropParams {
                    x: c.x,
                    y: c.y,
                    width: c.width,
                    height: c.height,
                });
                match load_image_content(&req.source, crop, Some(&working_dir)).await {
                    Ok((image, summary)) => {
                        blocks.push(ContentBlock::text(summary));
                        blocks.push(image);
                    }
                    Err(error) => blocks.push(ContentBlock::text(format!(
                        "[view_image could not load {}: {error}]",
                        req.source
                    ))),
                }
            }
            if total > MAX_IMAGES_PER_CELL {
                blocks.push(ContentBlock::text(format!(
                    "[view_image: {total} images requested; only the first {MAX_IMAGES_PER_CELL} are shown]"
                )));
            }
        }

        if is_error {
            CallToolResult::error(blocks)
        } else {
            CallToolResult::success(blocks)
        }
    }

    async fn session_changed(&self, session_id: &str, incarnation: Option<i64>) -> bool {
        match self.load_session(session_id).await {
            Some(session) => {
                incarnation.is_some_and(|micros| session.created_at.timestamp_micros() != micros)
            }
            // A load error here is ambiguous (deleted vs transient); only treat a
            // confirmed absence as a deletion.
            None => matches!(
                self.context
                    .session_manager
                    .session_exists(session_id)
                    .await,
                Ok(false)
            ),
        }
    }

    fn cached_listing(&self, session_id: &str) -> Option<String> {
        self.ns_cache.lock().unwrap().get(session_id).cloned()
    }

    fn has_slot(&self, session_id: &str) -> bool {
        self.sessions.lock().unwrap().contains_key(session_id)
    }

    async fn session_was_compacted(&self, session_id: &str) -> bool {
        if self.compacted.lock().unwrap().contains(session_id) {
            return true;
        }
        let Ok(session) = self
            .context
            .session_manager
            .get_session(session_id, true)
            .await
        else {
            return false;
        };
        let hit = session.conversation.is_some_and(|conversation| {
            conversation
                .messages()
                .iter()
                .any(crate::context_mgmt::is_compaction_continuation)
        });
        if hit {
            self.compacted
                .lock()
                .unwrap()
                .insert(session_id.to_string());
        }
        hit
    }
}

fn config_value<T: DeserializeOwned>(key: &str) -> Option<T> {
    Config::global().get_param(key).ok()
}

fn cell_timeout() -> Duration {
    let secs = config_value::<u64>(CELL_TIMEOUT_KEY).unwrap_or(DEFAULT_CELL_TIMEOUT_SECS);
    Duration::from_secs(secs.max(1))
}

fn discover_interpreter(path: Option<String>) -> Result<PathBuf, String> {
    if let Some(configured) = config_value::<String>(INTERPRETER_KEY) {
        return Ok(PathBuf::from(configured));
    }
    let path = path.unwrap_or_default();
    ["python3", "python"]
        .iter()
        .map(|name| format!("{name}{}", std::env::consts::EXE_SUFFIX))
        .find_map(|name| {
            std::env::split_paths(&path)
                .map(|dir| dir.join(&name))
                .find(|candidate| candidate.is_file())
        })
        .ok_or_else(|| {
            format!(
                "no python3 or python interpreter found on PATH; install Python 3.9+ or set {INTERPRETER_KEY}"
            )
        })
}

#[cfg(not(windows))]
fn login_shell_path(enabled: bool) -> Option<String> {
    enabled
        .then(super::developer::shell::resolve_login_shell_path)
        .flatten()
}

#[cfg(windows)]
fn login_shell_path(_enabled: bool) -> Option<String> {
    None
}

fn state_dir() -> PathBuf {
    crate::config::paths::Paths::data_dir().join("python-session")
}

/// `SessionManager` reuses a session id after the latest one is deleted, so key
/// the snapshot on the creation time too; a new session never inherits a deleted
/// conversation's variables (which could include secrets).
fn snapshot_path(session: &Session) -> PathBuf {
    state_dir().join(format!(
        "{}-{}.pkl",
        session.id,
        session.created_at.timestamp_micros()
    ))
}

/// Snapshots can hold source data and credentials; keep the directory
/// owner-only, matching the session database's protected storage.
fn prepare_state_dir() -> bool {
    let dir = state_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }
    true
}

fn format_outcome(
    outcome: &ExecOutcome,
    reset_notice: bool,
    restore_notice: Option<String>,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if reset_notice {
        parts.push(KERNEL_RESET_NOTICE.to_string());
    }
    if let Some(notice) = restore_notice {
        parts.push(notice);
    }
    if !outcome.stdout.trim().is_empty() {
        parts.push(outcome.stdout.trim_end().to_string());
    }
    if !outcome.stderr.trim().is_empty() {
        parts.push(format!("stderr:\n{}", outcome.stderr.trim_end()));
    }
    if let Some(error) = &outcome.error {
        parts.push(error.clone());
    } else if let Some(value) = &outcome.value {
        parts.push(format!("=> {value}"));
    }
    if outcome.interrupted && outcome.error.is_none() {
        parts.push("[cell was interrupted but completed anyway]".to_string());
    }
    if let Some(ms) = outcome.duration_ms.filter(|&ms| ms >= 5000) {
        parts.push(format!("[cell ran {:.1}s]", ms as f64 / 1000.0));
    }
    if parts.is_empty() {
        parts.push("(cell completed with no output)".to_string());
    }
    parts.join("\n")
}

#[async_trait]
impl McpClientTrait for PythonSessionClient {
    async fn list_tools(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        Ok(ListToolsResult {
            tools: Self::get_tools(),
            next_cursor: None,
            meta: None,
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        ctx: &ToolCallContext,
        name: &str,
        arguments: Option<JsonObject>,
        cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        match name {
            PYTHON_TOOL_NAME => match self.run_python(ctx, arguments, cancellation_token).await {
                Ok(output) => Ok(self.assemble_result(ctx, output).await),
                Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(error)])),
            },
            other => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Unknown tool: {other}"
            ))])),
        }
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        Some(&self.info)
    }

    /// The listing rides in the turn context, so every change to it invalidates the
    /// provider's prompt cache from that message onward. Before a compaction the model
    /// still sees its own `python` calls, so the re-anchor is only emitted once history
    /// has actually been compacted away. Sessions that never ran Python (no live
    /// kernel slot, no snapshot) return before the conversation is scanned. A session
    /// resumed in a new process has no kernel yet, so one is restored from the
    /// snapshot to list what came back.
    async fn get_moim(&self, session_id: &str) -> Option<String> {
        let session = self.load_session(session_id).await?;
        let has_snapshot = snapshot_path(&session).is_file();
        if (!self.has_slot(session_id) && !has_snapshot)
            || !self.session_was_compacted(session_id).await
        {
            return None;
        }

        let slot = self.session_slot(session_id);
        let listing = match slot.try_lock() {
            Ok(mut guard) => {
                if guard.kernel.is_none() && has_snapshot {
                    self.ensure_reaper();
                    let _ = self
                        .ensure_kernel(&mut guard, session_id, Some(session.working_dir))
                        .await;
                }
                match guard.kernel.as_mut() {
                    Some(kernel) => match kernel.namespace(NS_PROBE_TIMEOUT).await {
                        Ok(listing) => {
                            self.ns_cache
                                .lock()
                                .unwrap()
                                .insert(session_id.to_string(), listing.clone());
                            listing
                        }
                        Err(_) => self.cached_listing(session_id)?,
                    },
                    None => self.cached_listing(session_id)?,
                }
            }
            Err(_) => self.cached_listing(session_id)?,
        };

        let body = if listing.is_empty() {
            "(no variables defined yet)".to_string()
        } else {
            listing
        };
        Some(format!(
            "<python-session>\nVariables in your persistent Python session (they survive \
             compaction; reuse instead of recomputing):\n{body}\n</python-session>",
        ))
    }

    async fn update_working_dir(&self, new_dir: PathBuf) -> Result<(), Error> {
        let slots: Vec<(String, Arc<tokio::sync::Mutex<SessionSlot>>)> = self
            .sessions
            .lock()
            .unwrap()
            .iter()
            .map(|(id, slot)| (id.clone(), slot.clone()))
            .collect();
        for (session_id, slot) in slots {
            match slot.try_lock() {
                Ok(mut guard) => {
                    self.pending_dir.lock().unwrap().remove(&session_id);
                    if let Some(kernel) = guard.kernel.as_mut() {
                        if let Err(e) = kernel.chdir(&new_dir, CHDIR_TIMEOUT).await {
                            tracing::warn!("python session could not change directory: {e:#}");
                        }
                    }
                }
                // A cell holds the lock; apply the change before its next cell runs.
                Err(_) => {
                    self.pending_dir
                        .lock()
                        .unwrap()
                        .insert(session_id, new_dir.clone());
                }
            }
        }
        Ok(())
    }
}
