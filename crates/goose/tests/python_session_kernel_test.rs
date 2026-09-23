use std::path::{Path, PathBuf};
use std::time::Duration;

use goose::agents::platform_extensions::developer::python_session::kernel::{
    ExecOutcome, Kernel, KernelSpec,
};
use tokio_util::sync::CancellationToken;

fn python_available() -> bool {
    std::process::Command::new("python3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

macro_rules! require_python {
    () => {
        if !python_available() {
            eprintln!("skipping: python3 not available");
            return;
        }
    };
}

fn spec(working_dir: &Path, state_path: Option<&Path>) -> KernelSpec {
    KernelSpec {
        python: PathBuf::from("python3"),
        working_dir: working_dir.to_path_buf(),
        state_path: state_path.map(Path::to_path_buf),
        env: Vec::new(),
    }
}

async fn spawn_kernel() -> Kernel {
    Kernel::spawn(&spec(&std::env::temp_dir(), None))
        .await
        .expect("kernel should spawn")
}

async fn exec(kernel: &mut Kernel, code: &str) -> ExecOutcome {
    kernel
        .exec(code, Duration::from_secs(30), CancellationToken::new())
        .await
        .expect("exec should not kill the kernel")
}

#[tokio::test]
async fn namespace_persists_across_cells() {
    require_python!();
    let mut kernel = spawn_kernel().await;

    let outcome = exec(&mut kernel, "data = list(range(1000))\nlen(data)").await;
    assert_eq!(outcome.value.as_deref(), Some("1000"));
    assert!(outcome.error.is_none());

    let outcome = exec(&mut kernel, "sum(data[:10])").await;
    assert_eq!(outcome.value.as_deref(), Some("45"));

    let listing = kernel
        .namespace(Duration::from_secs(5))
        .await
        .expect("namespace probe");
    assert!(listing.contains("data: list len=1000"), "got: {listing}");
}

#[tokio::test]
async fn output_is_capped_with_marker() {
    require_python!();
    let mut kernel = spawn_kernel().await;

    let outcome = exec(&mut kernel, "print('x' * 100_000)").await;
    assert!(
        outcome.stdout.len() < 20_000,
        "len: {}",
        outcome.stdout.len()
    );
    assert!(outcome.stdout.contains("output truncated"));
}

#[cfg(unix)]
#[tokio::test]
async fn timeout_interrupts_cell_but_keeps_namespace() {
    require_python!();
    let mut kernel = spawn_kernel().await;

    exec(&mut kernel, "marker = 'alive'").await;
    let outcome = kernel
        .exec(
            "import time\ntime.sleep(60)",
            Duration::from_secs(1),
            CancellationToken::new(),
        )
        .await
        .expect("interrupt should not kill the kernel");
    assert!(outcome.interrupted);
    assert!(outcome
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("KeyboardInterrupt"));

    let outcome = exec(&mut kernel, "marker").await;
    assert_eq!(outcome.value.as_deref(), Some("'alive'"));
}

#[tokio::test]
async fn kernel_death_is_reported_as_error() {
    require_python!();
    let mut kernel = spawn_kernel().await;

    let result = kernel
        .exec(
            "import os\nos._exit(9)",
            Duration::from_secs(5),
            CancellationToken::new(),
        )
        .await;
    assert!(result.is_err(), "exec against a dead kernel must error");
}

#[tokio::test]
async fn namespace_survives_process_restart_via_state_snapshot() {
    require_python!();
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state.pkl");
    let spec = spec(dir.path(), Some(&state));

    let mut kernel = Kernel::spawn(&spec).await.expect("kernel should spawn");
    assert!(kernel.restored_names().is_empty());
    exec(
        &mut kernel,
        "totals = {'a': 1, 'b': 2}\n_scratch = [3]\nimport socket\nsock = socket.socket()",
    )
    .await;
    kernel.kill();

    let mut revived = Kernel::spawn(&spec).await.expect("kernel should respawn");
    assert!(
        revived.restored_names().contains(&"totals".to_string()),
        "picklable variable should be restored, got: {:?}",
        revived.restored_names()
    );
    assert!(
        !revived.restored_names().contains(&"sock".to_string()),
        "unpicklable variable must be skipped, not fail the snapshot"
    );
    let outcome = exec(&mut revived, "(totals['b'], _scratch)").await;
    assert_eq!(outcome.value.as_deref(), Some("(2, [3])"));
}

#[tokio::test]
async fn restore_keeps_aliases_and_drops_only_unloadable_variables() {
    require_python!();
    let dir = tempfile::tempdir().unwrap();
    let modules = dir.path().join("modules");
    std::fs::create_dir(&modules).unwrap();
    std::fs::write(
        modules.join("gonemod.py"),
        "class Thing:\n    pass\n\nclass Bag(list):\n    pass\n\nclass Table(dict):\n    pass\n",
    )
    .unwrap();
    let state = dir.path().join("state.pkl");
    let spec = spec(dir.path(), Some(&state));

    // The module is importable only through this process's sys.path, so the
    // respawned kernel cannot resolve `obj` while `a`, `b`, `n`, and the `js`
    // alias still come back.
    let mut kernel = Kernel::spawn(&spec).await.expect("kernel should spawn");
    exec(
        &mut kernel,
        &format!(
            "import sys\nsys.path.insert(0, {:?})\nimport gonemod\nimport json as js\nobj = gonemod.Thing()\nbag = gonemod.Bag([1])\ntable = gonemod.Table(k=1)\na = [1, 2]\nb = a\nn = 5\nbig = 10**5000",
            modules.to_str().unwrap()
        ),
    )
    .await;
    kernel.kill();

    let mut revived = Kernel::spawn(&spec).await.expect("kernel should respawn");
    assert_eq!(
        revived.dropped_names(),
        &[
            "gonemod".to_string(),
            "obj".to_string(),
            "bag".to_string(),
            "table".to_string()
        ]
    );
    let outcome = exec(&mut revived, "(a is b, n, js.dumps(n), big > 0)").await;
    assert_eq!(outcome.value.as_deref(), Some("(True, 5, '5', True)"));
}

#[cfg(unix)]
#[tokio::test]
async fn shell_capture_is_bounded_per_stream() {
    require_python!();
    let mut kernel = spawn_kernel().await;

    // 70 MB exceeds the 64 MiB per-stream capture cap; the head is kept on the
    // result and the remainder is reported, not buffered.
    let outcome = exec(
        &mut kernel,
        r#"r = sh("yes aaaaaaaa | head -c 70000000")
(len(r.out) < 68 * 1024 * 1024, r.out.startswith("aaaa"), "bytes of stdout dropped" in r.out, r.code)"#,
    )
    .await;
    assert_eq!(
        outcome.value.as_deref(),
        Some("(True, True, True, 0)"),
        "error: {:?}",
        outcome.error
    );
}

#[cfg(unix)]
fn process_running(pattern: &str) -> bool {
    std::process::Command::new("pgrep")
        .args(["-f", pattern])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(unix)]
#[tokio::test]
async fn background_command_holding_the_pipes_is_reaped_with_the_kernel() {
    require_python!();
    let mut kernel = spawn_kernel().await;
    let marker = format!("sleep 300.{}", std::process::id() % 100_000);

    let outcome = exec(
        &mut kernel,
        &format!("r = sh(\"{marker} &\")\n\"still open\" in r.out"),
    )
    .await;
    assert_eq!(
        outcome.value.as_deref(),
        Some("True"),
        "error: {:?}",
        outcome.error
    );
    assert!(
        process_running(&marker),
        "the background command should outlive the shell"
    );

    // The driver's SIGTERM handler reaps the shell group that still holds the pipes.
    kernel.stop();
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while process_running(&marker) {
        assert!(
            std::time::Instant::now() < deadline,
            "background command was orphaned by the kernel stop"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn image_requests_are_capped_in_the_driver() {
    require_python!();
    let mut kernel = spawn_kernel().await;

    let outcome = exec(
        &mut kernel,
        "for i in range(20):\n    view_image('/tmp/shot-%d.png' % i)",
    )
    .await;
    assert_eq!(outcome.images.len(), 8);
    assert_eq!(outcome.images_dropped, 12);
    assert!(outcome.images[0].source.ends_with("shot-0.png"));
}

#[tokio::test]
async fn surrogate_output_does_not_hang_the_cell() {
    require_python!();
    let mut kernel = spawn_kernel().await;

    // Bytes decoded with surrogateescape (e.g. inspecting a non-UTF-8 file or
    // filename) yield lone surrogates. The driver must neutralize them before the
    // response is parsed; otherwise the cell would wedge until its timeout.
    let outcome = kernel
        .exec(
            r#"print("start", b"\xff\xfe".decode("utf-8", "surrogateescape"), "end")"#,
            Duration::from_secs(10),
            CancellationToken::new(),
        )
        .await
        .expect("surrogate output must not kill the kernel");

    assert!(outcome.error.is_none());
    assert!(outcome.stdout.contains("start"));
    assert!(outcome.stdout.contains("end"));

    // The kernel is still healthy for the next cell.
    let outcome = exec(&mut kernel, "1 + 1").await;
    assert_eq!(outcome.value.as_deref(), Some("2"));
}
