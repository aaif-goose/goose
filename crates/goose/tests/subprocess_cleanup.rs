#![cfg(target_os = "linux")]

use goose::subprocess::{configure_subprocess, spawn_long_lived_mcp_subprocess};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const HELPER_ENV: &str = "GOOSE_SUBPROCESS_PARENT_DEATH_HELPER";
const THREAD_HELPER_ENV: &str = "GOOSE_SUBPROCESS_THREAD_DEATH_HELPER";

struct HelperProcess(Child);

impl Drop for HelperProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[ctor::ctor]
unsafe fn maybe_run_helper() {
    if std::env::var_os(THREAD_HELPER_ENV).is_some() {
        run_thread_death_helper();
    }

    if std::env::var_os(HELPER_ENV).is_none() {
        return;
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    let pid = runtime.block_on(async {
        let mut command = tokio::process::Command::new("sleep");
        command.arg("30");
        command.stdin(Stdio::null());
        command.stdout(Stdio::null());
        command.stderr(Stdio::null());
        configure_subprocess(&mut command);

        let child = command.spawn().expect("spawn child");
        let pid = child.id().expect("child pid");
        std::mem::forget(child);
        pid
    });

    println!("{pid}");
    std::io::stdout().flush().expect("flush pid");

    unsafe {
        libc::_exit(0);
    }
}

fn run_thread_death_helper() {
    let (tx, rx) = mpsc::channel();
    let spawn_thread = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        let pid = runtime.block_on(async {
            let mut command = tokio::process::Command::new("sleep");
            command.arg("30");
            command.stdin(Stdio::null());
            command.stdout(Stdio::null());
            command.stderr(Stdio::null());
            let (child, _) = spawn_long_lived_mcp_subprocess(command)
                .await
                .expect("spawn child");
            let pid = child.id().expect("child pid");
            std::mem::forget(child);
            pid
        });

        tx.send(pid).expect("send child pid");
    });

    spawn_thread.join().expect("spawn thread");
    let child_pid = rx.recv().expect("child pid");
    std::thread::sleep(Duration::from_millis(500));

    if !process_is_running(child_pid) {
        eprintln!("child process {child_pid} exited after spawning thread exit");
        unsafe {
            libc::_exit(1);
        }
    }

    println!("{child_pid}");
    std::io::stdout().flush().expect("flush pid");
    loop {
        std::thread::park();
    }
}

#[test]
fn child_process_exits_when_parent_process_dies() {
    let current_exe = std::env::current_exe().expect("current test binary");
    let mut helper = Command::new(current_exe)
        .env(HELPER_ENV, "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn helper");

    let pid_line = {
        let stdout = helper.stdout.take().expect("helper stdout");
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        reader.read_line(&mut line).expect("read child pid");
        line
    };

    let child_pid = pid_line.trim().parse::<u32>().expect("parse child pid");

    let status = helper.wait().expect("wait for helper");
    assert!(status.success(), "helper exited unsuccessfully: {status}");

    let deadline = Instant::now() + Duration::from_secs(5);
    while process_exists(child_pid) {
        assert!(
            Instant::now() < deadline,
            "child process {child_pid} still exists after helper exit"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn long_lived_child_process_survives_spawning_thread_exit() {
    let current_exe = std::env::current_exe().expect("current test binary");
    let mut helper = HelperProcess(
        Command::new(current_exe)
            .env(THREAD_HELPER_ENV, "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn helper"),
    );

    let stdout = helper.0.stdout.take().expect("helper stdout");
    let (pid_tx, pid_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let pid = BufReader::new(stdout)
            .lines()
            .next()
            .ok_or("helper exited without reporting a child pid")
            .and_then(|line| line.map_err(|_| "failed to read helper child pid"))
            .and_then(|line| line.parse::<u32>().map_err(|_| "invalid helper child pid"));
        let _ = pid_tx.send(pid);
    });
    let child_pid = pid_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("timed out waiting for helper child pid")
        .expect("helper child pid");

    assert!(
        process_is_running(child_pid),
        "child process {child_pid} exited after spawning thread exit"
    );

    unsafe {
        libc::kill(helper.0.id() as libc::pid_t, libc::SIGKILL);
    }
    let status = helper.0.wait().expect("wait for helper");
    assert!(
        !status.success(),
        "helper should have been killed: {status}"
    );

    let deadline = Instant::now() + Duration::from_secs(5);
    while process_is_running(child_pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        !process_is_running(child_pid),
        "child process {child_pid} survived parent process death"
    );
}

fn process_exists(pid: u32) -> bool {
    PathBuf::from(format!("/proc/{pid}")).exists()
}

fn process_is_running(pid: u32) -> bool {
    match process_state(pid) {
        Some('Z') | None => false,
        Some(_) => true,
    }
}

fn process_state(pid: u32) -> Option<char> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let (_, after_name) = stat.rsplit_once(") ")?;
    after_name.chars().next()
}
