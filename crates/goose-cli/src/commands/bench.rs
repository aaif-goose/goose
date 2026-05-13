use anyhow::{bail, Context, Result};
use clap::ValueEnum;
use goose::config::paths::Paths;
use goose_mcp::mcp_server_runner::serve;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, Content, ErrorData, Implementation, InitializeResult, ServerCapabilities,
        ServerInfo,
    },
    schemars,
    schemars::JsonSchema,
    tool, tool_handler, tool_router, ServerHandler,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

const TERMINAL_BENCH_AGENT_IMPORT_PATH: &str =
    "goose_terminal_bench.goose_external:GooseExternalAgent";
const TERMINAL_BENCH_ADAPTER_FILES: &[(&str, &str)] = &[
    (
        "__init__.py",
        include_str!("../../../../evals/terminal-bench/goose_terminal_bench/__init__.py"),
    ),
    (
        "docker_util.py",
        include_str!("../../../../evals/terminal-bench/goose_terminal_bench/docker_util.py"),
    ),
    (
        "goose_external.py",
        include_str!("../../../../evals/terminal-bench/goose_terminal_bench/goose_external.py"),
    ),
];
const OUTPUT_LIMIT_BYTES: usize = 50_000;
const DEFAULT_SHELL_TIMEOUT_SECS: u64 = 300;
const FILE_OPERATION_TIMEOUT_SECS: u64 = 60;

#[derive(Debug)]
pub struct TerminalBenchRunOptions {
    pub model: String,
    pub dataset: String,
    pub tasks: Vec<String>,
    pub trials: u32,
    pub concurrency: u32,
    pub max_turns: Option<u32>,
    pub workdir: Option<String>,
    pub python_index_url: Option<String>,
    pub jobs_dir: Option<PathBuf>,
    pub config_dir: Option<PathBuf>,
    pub job_name: Option<String>,
    pub force_build: bool,
    pub dry_run: bool,
    pub goose_binary: Option<PathBuf>,
}

pub async fn handle_terminal_bench_run(options: TerminalBenchRunOptions) -> Result<()> {
    let adapter_pythonpath = write_terminal_bench_adapter(&default_adapter_dir())?;

    let config = build_terminal_bench_config(&options)?;
    let config_dir = options
        .config_dir
        .clone()
        .unwrap_or_else(default_config_dir);
    let config_path = write_terminal_bench_config(&config, &config_dir)?;
    let pythonpath = pythonpath_with_dir(&adapter_pythonpath);
    let command_display = format!("harbor run -c {}", config_path.display());

    if options.dry_run {
        println!("Wrote Harbor config: {}", config_path.display());
        println!(
            "Wrote Harbor adapter package: {}",
            adapter_pythonpath.display()
        );
        println!("PYTHONPATH: {pythonpath}");
        println!("Command:");
        println!("{command_display}");
        return Ok(());
    }

    let mut command = std::process::Command::new("harbor");
    command.args(["run", "-c"]).arg(&config_path);
    command.env("PYTHONPATH", pythonpath);

    let status = command
        .status()
        .context("Failed to run `harbor`. Install Harbor and ensure it is on PATH.")?;
    if !status.success() {
        bail!("Harbor exited with status {status}");
    }
    Ok(())
}

fn build_terminal_bench_config(options: &TerminalBenchRunOptions) -> Result<Value> {
    if !options.model.contains('/') {
        bail!("Model must be in provider/model form, for example databricks/my-model");
    }
    if options.trials == 0 {
        bail!("--trials must be at least 1");
    }
    if options.concurrency == 0 {
        bail!("--concurrency must be at least 1");
    }

    let (dataset_name, dataset_version) = split_dataset(&options.dataset);
    let mut dataset = serde_json::Map::from_iter([("name".to_string(), json!(dataset_name))]);
    if let Some(version) = dataset_version {
        dataset.insert("version".to_string(), json!(version));
    }
    if !options.tasks.is_empty() {
        dataset.insert("task_names".to_string(), json!(options.tasks));
    }

    let goose_binary = options
        .goose_binary
        .clone()
        .map(Ok)
        .unwrap_or_else(std::env::current_exe)
        .context("Failed to determine current Goose executable")?;
    let mut agent_kwargs = serde_json::Map::from_iter([(
        "goose_binary".to_string(),
        json!(goose_binary.to_string_lossy()),
    )]);
    if let Some(workdir) = &options.workdir {
        agent_kwargs.insert("workdir".to_string(), json!(workdir));
    }
    if let Some(max_turns) = options.max_turns {
        agent_kwargs.insert("max_turns".to_string(), json!(max_turns));
    }

    let python_env = python_index_env(resolve_python_index_url(
        options.python_index_url.as_deref(),
    ));
    let job_name = match &options.job_name {
        Some(name) => validate_job_name(name)?.to_string(),
        None => default_job_name(&options.model, &options.dataset),
    };

    Ok(json!({
        "job_name": job_name,
        "jobs_dir": options.jobs_dir.clone().unwrap_or_else(default_jobs_dir).to_string_lossy(),
        "n_attempts": options.trials,
        "n_concurrent_trials": options.concurrency,
        "environment": {
            "type": "docker",
            "force_build": options.force_build,
            "delete": true,
            "env": python_env,
        },
        "verifier": {
            "env": python_env,
        },
        "agents": [
            {
                "import_path": TERMINAL_BENCH_AGENT_IMPORT_PATH,
                "model_name": options.model,
                "kwargs": agent_kwargs,
            }
        ],
        "datasets": [Value::Object(dataset)],
    }))
}

fn write_terminal_bench_adapter(adapter_dir: &Path) -> Result<PathBuf> {
    let package_dir = adapter_dir.join("goose_terminal_bench");
    std::fs::create_dir_all(&package_dir)
        .with_context(|| format!("Failed to create {}", package_dir.display()))?;

    for (file_name, contents) in TERMINAL_BENCH_ADAPTER_FILES {
        let path = package_dir.join(file_name);
        std::fs::write(&path, contents)
            .with_context(|| format!("Failed to write {}", path.display()))?;
    }

    Ok(adapter_dir.to_path_buf())
}

fn write_terminal_bench_config(config: &Value, config_dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(config_dir)
        .with_context(|| format!("Failed to create {}", config_dir.display()))?;
    let job_name = config
        .get("job_name")
        .and_then(Value::as_str)
        .context("Terminal Bench config is missing job_name")?;
    let path = config_dir.join(format!("{job_name}.json"));
    let contents = serde_json::to_string_pretty(config)?;
    std::fs::write(&path, format!("{contents}\n"))
        .with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(path)
}

fn default_jobs_dir() -> PathBuf {
    Paths::in_data_dir("bench/terminal-bench/jobs")
}

fn default_config_dir() -> PathBuf {
    Paths::in_state_dir("bench/terminal-bench/configs")
}

fn default_adapter_dir() -> PathBuf {
    Paths::in_state_dir("bench/terminal-bench/adapter")
}

fn split_dataset(value: &str) -> (&str, Option<&str>) {
    value
        .rsplit_once('@')
        .map_or((value, None), |(name, version)| (name, Some(version)))
}

fn default_job_name(model: &str, dataset: &str) -> String {
    let safe_model = model.replace(['/', ':'], "-");
    let safe_dataset = dataset.replace(['/', '@', ':'], "-");
    let timestamp = chrono::Local::now().format("%Y-%m-%d__%H-%M-%S");
    format!("goose-{safe_dataset}-{safe_model}-{timestamp}")
}

fn validate_job_name(job_name: &str) -> Result<&str> {
    let mut chars = job_name.chars();
    let Some(first) = chars.next() else {
        bail!("Job name cannot be empty");
    };
    if !first.is_ascii_alphanumeric()
        || !chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    {
        bail!(
            "Job name must start with a letter or number and contain only letters, numbers, dots, underscores, and hyphens"
        );
    }
    Ok(job_name)
}

fn resolve_python_index_url(explicit_url: Option<&str>) -> Option<String> {
    explicit_url.map(str::to_string).or_else(|| {
        std::env::var("UV_DEFAULT_INDEX")
            .or_else(|_| std::env::var("PIP_INDEX_URL"))
            .or_else(|_| std::env::var("UV_INDEX_URL"))
            .ok()
    })
}

fn python_index_env(index_url: Option<String>) -> serde_json::Map<String, Value> {
    let mut env = serde_json::Map::new();
    if let Some(index_url) = index_url {
        env.insert("PIP_INDEX_URL".to_string(), json!(index_url));
        env.insert("UV_DEFAULT_INDEX".to_string(), json!(index_url));
        env.insert("UV_INDEX_URL".to_string(), json!(index_url));
    }
    env
}

fn pythonpath_with_dir(dir: &Path) -> String {
    let dir = dir.to_string_lossy();
    match std::env::var("PYTHONPATH") {
        Ok(existing) if !existing.is_empty() => {
            format!("{dir}{}{existing}", pythonpath_separator())
        }
        _ => dir.into_owned(),
    }
}

fn pythonpath_separator() -> char {
    if cfg!(windows) {
        ';'
    } else {
        ':'
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum BenchToolProfile {
    Developer,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum BenchToolTarget {
    Docker,
}

pub async fn handle_tool_server(
    profile: BenchToolProfile,
    target: BenchToolTarget,
    container: String,
    workdir: String,
) -> Result<()> {
    match (profile, target) {
        (BenchToolProfile::Developer, BenchToolTarget::Docker) => {
            serve(DeveloperBenchServer::new(DockerTarget::new(
                container, workdir,
            )))
            .await
        }
    }
}

#[derive(Clone)]
struct DockerTarget {
    container: String,
    workdir: String,
}

impl DockerTarget {
    fn new(container: String, workdir: String) -> Self {
        Self { container, workdir }
    }

    fn resolve_path(&self, path: &str) -> String {
        normalize_container_path(path, &self.workdir)
    }

    async fn exec_shell(&self, command: &str, timeout_secs: Option<u64>) -> ShellResult {
        if command.trim().is_empty() {
            return ShellResult::error("Command cannot be empty.".to_string(), None);
        }

        let mut docker = tokio::process::Command::new("docker");
        docker
            .kill_on_drop(true)
            .args([
                "exec",
                "-w",
                &self.workdir,
                &self.container,
                "/bin/sh",
                "-lc",
                command,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let child = match docker.spawn() {
            Ok(child) => child,
            Err(error) => {
                return ShellResult::error(
                    format!(
                        "Failed to run docker exec for {}: {}",
                        self.container, error
                    ),
                    None,
                )
            }
        };

        let wait = child.wait_with_output();
        let timeout_secs = timeout_secs.unwrap_or(DEFAULT_SHELL_TIMEOUT_SECS);
        let output = match tokio::time::timeout(Duration::from_secs(timeout_secs), wait).await {
            Ok(output) => output,
            Err(_) => {
                return ShellResult {
                    stdout: String::new(),
                    stderr: format!("Command timed out after {timeout_secs} seconds"),
                    exit_code: None,
                    timed_out: true,
                    output_truncated: false,
                }
            }
        };

        match output {
            Ok(output) => {
                let (stdout, stdout_truncated) =
                    truncate_output(&String::from_utf8_lossy(&output.stdout));
                let (stderr, stderr_truncated) =
                    truncate_output(&String::from_utf8_lossy(&output.stderr));
                ShellResult {
                    stdout,
                    stderr,
                    exit_code: output.status.code(),
                    timed_out: false,
                    output_truncated: stdout_truncated || stderr_truncated,
                }
            }
            Err(error) => ShellResult::error(
                format!("Failed to collect docker exec output: {error}"),
                None,
            ),
        }
    }

    async fn read_file(&self, path: &str) -> Result<String, String> {
        let path = self.resolve_path(path);
        let mut command = tokio::process::Command::new("docker");
        command
            .kill_on_drop(true)
            .args(["exec", &self.container, "cat", &path]);
        let output = tokio::time::timeout(
            Duration::from_secs(FILE_OPERATION_TIMEOUT_SECS),
            command.output(),
        )
        .await
        .map_err(|_| {
            format!("Timed out reading {path} after {FILE_OPERATION_TIMEOUT_SECS} seconds")
        })?
        .map_err(|error| format!("Failed to run docker exec: {error}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(if stderr.is_empty() {
                format!("Failed to read {path}")
            } else {
                stderr
            });
        }

        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    async fn write_file(&self, path: &str, content: &str) -> Result<(), String> {
        let path = self.resolve_path(path);
        let parent = container_parent(&path);
        let parent = shell_quote(&parent)?;
        let path = shell_quote(&path)?;
        let script = format!("mkdir -p -- {parent} && cat > {path}",);

        let mut child = tokio::process::Command::new("docker")
            .args(["exec", "-i", &self.container, "/bin/sh", "-lc", &script])
            .kill_on_drop(true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("Failed to run docker exec: {error}"))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(content.as_bytes())
                .await
                .map_err(|error| format!("Failed to stream file contents: {error}"))?;
        }

        let output = tokio::time::timeout(
            Duration::from_secs(FILE_OPERATION_TIMEOUT_SECS),
            child.wait_with_output(),
        )
        .await
        .map_err(|_| format!("Timed out writing file after {FILE_OPERATION_TIMEOUT_SECS} seconds"))?
        .map_err(|error| format!("Failed to collect docker exec output: {error}"))?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            Err(if stderr.is_empty() {
                format!("Failed to write {path}")
            } else {
                stderr
            })
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ShellParams {
    pub command: String,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ShellResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub output_truncated: bool,
}

impl ShellResult {
    fn error(stderr: String, exit_code: Option<i32>) -> Self {
        Self {
            stdout: String::new(),
            stderr,
            exit_code,
            timed_out: false,
            output_truncated: false,
        }
    }

    fn render(&self) -> String {
        let mut parts = Vec::new();
        if !self.stdout.is_empty() {
            parts.push(self.stdout.clone());
        }
        if !self.stderr.is_empty() {
            parts.push(self.stderr.clone());
        }

        let mut rendered = parts.join("\n");
        if self.output_truncated {
            rendered.push_str(&format!(
                "\n\nOutput truncated to {OUTPUT_LIMIT_BYTES} bytes."
            ));
        }
        if self.timed_out {
            rendered.push_str("\n\nCommand timed out.");
        } else if self.exit_code.is_some_and(|code| code != 0) {
            rendered.push_str(&format!(
                "\n\nCommand exited with code {}",
                self.exit_code.unwrap()
            ));
        }
        rendered
    }

    fn into_tool_result(self) -> CallToolResult {
        let is_error = self.timed_out || self.exit_code.is_none_or(|code| code != 0);
        let rendered = self.render();
        let mut result = if is_error {
            CallToolResult::error(vec![Content::text(rendered).with_priority(0.0)])
        } else {
            CallToolResult::success(vec![Content::text(rendered).with_priority(0.0)])
        };
        result.structured_content = serde_json::to_value(&self).ok();
        result
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FileWriteParams {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FileEditParams {
    pub path: String,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TreeParams {
    pub path: String,
    #[serde(default = "default_depth")]
    pub depth: u32,
}

fn default_depth() -> u32 {
    2
}

#[derive(Clone)]
struct DeveloperBenchServer {
    target: DockerTarget,
    tool_router: ToolRouter<Self>,
}

#[tool_router(router = tool_router)]
impl DeveloperBenchServer {
    fn new(target: DockerTarget) -> Self {
        Self {
            target,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        name = "shell",
        description = "Execute a shell command inside the benchmark task container. Relative paths resolve under the configured workdir, and absolute paths refer to the container filesystem."
    )]
    async fn shell(
        &self,
        params: Parameters<ShellParams>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let params = params.0;
        Ok(self
            .target
            .exec_shell(&params.command, params.timeout_secs)
            .await
            .into_tool_result())
    }

    #[tool(
        name = "write",
        description = "Create a new file or overwrite an existing file inside the benchmark task container. Creates parent directories if needed."
    )]
    async fn write(
        &self,
        params: Parameters<FileWriteParams>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let params = params.0;
        match self.target.write_file(&params.path, &params.content).await {
            Ok(()) => {
                let line_count = params.content.lines().count();
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Wrote {} ({} lines)",
                    params.path, line_count
                ))
                .with_priority(0.0)]))
            }
            Err(error) => Ok(error_result(format!(
                "Failed to write {}: {}",
                params.path, error
            ))),
        }
    }

    #[tool(
        name = "edit",
        description = "Edit a file inside the benchmark task container by finding and replacing text. The before text must match exactly and uniquely."
    )]
    async fn edit(
        &self,
        params: Parameters<FileEditParams>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let params = params.0;
        let content = match self.target.read_file(&params.path).await {
            Ok(content) => content,
            Err(error) => {
                return Ok(error_result(format!(
                    "Failed to read {}: {}",
                    params.path, error
                )))
            }
        };

        let new_content = match string_replace(&content, &params.before, &params.after) {
            Ok(content) => content,
            Err(error) => return Ok(error_result(error)),
        };

        match self.target.write_file(&params.path, &new_content).await {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Edited {} ({} lines -> {} lines)",
                params.path,
                params.before.lines().count(),
                params.after.lines().count()
            ))
            .with_priority(0.0)])),
            Err(error) => Ok(error_result(format!(
                "Failed to write {}: {}",
                params.path, error
            ))),
        }
    }

    #[tool(
        name = "tree",
        description = "List a directory tree inside the benchmark task container with line counts when available."
    )]
    async fn tree(
        &self,
        params: Parameters<TreeParams>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let params = params.0;
        let root = self.target.resolve_path(&params.path);
        let max_depth = if params.depth == 0 {
            String::new()
        } else {
            format!("-maxdepth {}", params.depth)
        };
        let script = format!(
            r#"
root={root}
if [ ! -e "$root" ]; then
  echo "Path does not exist: $root" >&2
  exit 1
fi
if [ ! -d "$root" ]; then
  echo "Path is not a directory: $root" >&2
  exit 1
fi
find "$root" {max_depth} \( -name .git -o -name __pycache__ -o -name .pytest_cache \) -prune -o -print | sort | awk 'NR <= 500'
"#,
            root = match shell_quote(&root) {
                Ok(root) => root,
                Err(error) => return Ok(error_result(error)),
            },
            max_depth = max_depth,
        );

        let output = self.target.exec_shell(&script, Some(30)).await;
        if output.exit_code != Some(0) {
            return Ok(output.into_tool_result());
        }

        let mut rendered = String::new();
        for line in output.stdout.lines().filter(|line| *line != root) {
            let display = line
                .strip_prefix(&(root.clone() + "/"))
                .unwrap_or(line)
                .to_string();
            if display.is_empty() {
                continue;
            }
            rendered.push_str(&display);
            rendered.push('\n');
        }
        if rendered.is_empty() {
            rendered.push_str("(empty directory)");
        }

        Ok(CallToolResult::success(vec![
            Content::text(rendered).with_priority(0.0)
        ]))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for DeveloperBenchServer {
    fn get_info(&self) -> ServerInfo {
        InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("developer", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Use these developer tools to operate inside the benchmark task container. \
                 Relative paths resolve under the configured workdir; absolute paths refer to \
                 paths inside the container."
                    .to_string(),
            )
    }
}

fn error_result(message: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![Content::text(message.into()).with_priority(0.0)])
}

fn string_replace(content: &str, before: &str, after: &str) -> Result<String, String> {
    let matches: Vec<_> = content.match_indices(before).collect();
    match matches.len() {
        0 => {
            let preview = content.lines().take(20).collect::<Vec<_>>().join("\n");
            Err(format!(
                "No match found for the specified text.\n\nFile preview:\n```\n{preview}\n```"
            ))
        }
        1 => Ok(content.replacen(before, after, 1)),
        count => Err(format!(
            "Found {count} matches. Please provide more context to identify a unique match."
        )),
    }
}

fn truncate_output(value: &str) -> (String, bool) {
    if value.len() <= OUTPUT_LIMIT_BYTES {
        return (value.to_string(), false);
    }

    let mut bytes = 0;
    let truncated = value
        .chars()
        .take_while(|character| {
            let next_len = bytes + character.len_utf8();
            if next_len > OUTPUT_LIMIT_BYTES {
                false
            } else {
                bytes = next_len;
                true
            }
        })
        .collect();

    (truncated, true)
}

fn normalize_container_path(path: &str, workdir: &str) -> String {
    let raw = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("{}/{}", workdir.trim_end_matches('/'), path)
    };

    let absolute = raw.starts_with('/');
    let mut parts = Vec::new();
    for part in raw.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }

    let normalized = parts.join("/");
    if absolute {
        format!("/{normalized}")
    } else if normalized.is_empty() {
        ".".to_string()
    } else {
        normalized
    }
}

fn container_parent(path: &str) -> String {
    match path.rsplit_once('/') {
        Some(("", _)) => "/".to_string(),
        Some((parent, _)) => parent.to_string(),
        None => ".".to_string(),
    }
}

fn shell_quote(value: &str) -> Result<String, String> {
    shlex::try_quote(value)
        .map(|value| value.into_owned())
        .map_err(|error| format!("Failed to shell-quote value: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terminal_bench_options() -> TerminalBenchRunOptions {
        TerminalBenchRunOptions {
            model: "databricks/databricks-gpt-5-5".to_string(),
            dataset: "terminal-bench@2.0".to_string(),
            tasks: vec!["fix-git".to_string()],
            trials: 2,
            concurrency: 1,
            max_turns: Some(25),
            workdir: None,
            python_index_url: Some("https://example.test/simple".to_string()),
            jobs_dir: Some(PathBuf::from("/tmp/jobs")),
            config_dir: Some(PathBuf::from("/tmp/configs")),
            job_name: Some("example-job".to_string()),
            force_build: true,
            dry_run: false,
            goose_binary: Some(PathBuf::from("/tmp/goose")),
        }
    }

    #[test]
    fn builds_terminal_bench_config() {
        let options = terminal_bench_options();
        let config = build_terminal_bench_config(&options).unwrap();

        assert_eq!(config["job_name"], "example-job");
        assert_eq!(config["jobs_dir"], "/tmp/jobs");
        assert_eq!(config["n_attempts"], 2);
        assert_eq!(config["n_concurrent_trials"], 1);
        assert_eq!(config["environment"]["force_build"], true);
        assert_eq!(
            config["environment"]["env"]["PIP_INDEX_URL"],
            "https://example.test/simple"
        );
        assert_eq!(config["datasets"][0]["name"], "terminal-bench");
        assert_eq!(config["datasets"][0]["version"], "2.0");
        assert_eq!(config["datasets"][0]["task_names"][0], "fix-git");
        assert_eq!(
            config["agents"][0]["import_path"],
            TERMINAL_BENCH_AGENT_IMPORT_PATH
        );
        assert_eq!(config["agents"][0]["kwargs"]["goose_binary"], "/tmp/goose");
        assert_eq!(config["agents"][0]["kwargs"]["max_turns"], 25);
    }

    #[test]
    fn materializes_terminal_bench_adapter_package() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pythonpath = write_terminal_bench_adapter(temp_dir.path()).unwrap();

        assert_eq!(pythonpath, temp_dir.path());
        assert!(temp_dir
            .path()
            .join("goose_terminal_bench/__init__.py")
            .exists());
        assert!(temp_dir
            .path()
            .join("goose_terminal_bench/docker_util.py")
            .exists());
        assert!(temp_dir
            .path()
            .join("goose_terminal_bench/goose_external.py")
            .exists());
    }

    #[test]
    fn rejects_invalid_terminal_bench_inputs() {
        let mut options = terminal_bench_options();
        options.job_name = Some("../surprise".to_string());
        assert!(build_terminal_bench_config(&options).is_err());

        let mut options = terminal_bench_options();
        options.model = "missing-provider-prefix".to_string();
        assert!(build_terminal_bench_config(&options).is_err());
    }

    #[test]
    fn normalizes_relative_paths_under_workdir() {
        assert_eq!(
            normalize_container_path("answer.txt", "/app"),
            "/app/answer.txt"
        );
        assert_eq!(
            normalize_container_path("../tmp/answer.txt", "/app"),
            "/tmp/answer.txt"
        );
        assert_eq!(
            normalize_container_path("/tests/check.py", "/app"),
            "/tests/check.py"
        );
    }

    #[test]
    fn edit_requires_unique_match() {
        let content = "same\nsame\n";
        let err = string_replace(content, "same", "different").unwrap_err();
        assert!(err.contains("Found 2 matches"));
    }

    #[test]
    fn shell_quote_handles_spaces() {
        assert_eq!(
            shell_quote("/tmp/path with spaces").unwrap(),
            "'/tmp/path with spaces'"
        );
    }

    #[test]
    fn shell_quote_rejects_nul_bytes() {
        assert!(shell_quote("bad\0path").is_err());
    }
}
