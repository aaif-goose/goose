#![cfg(target_os = "linux")]

use goose::subprocess::configure_subprocess;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const HELPER_ENV: &str = "GOOSE_SUBPROCESS_PARENT_DEATH_HELPER";

#[ctor::ctor]
fn maybe_run_helper() {
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

fn process_exists(pid: u32) -> bool {
    PathBuf::from(format!("/proc/{pid}")).exists()
}
