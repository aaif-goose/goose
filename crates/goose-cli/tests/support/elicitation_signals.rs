#![cfg(unix)]

use super::*;
use std::fs::File;
use std::future::{poll_fn, Future};
use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::task::Poll;
use std::time::{Duration, Instant};
use test_case::test_case;
use tokio_util::sync::CancellationToken;

const CHILD_MODE: &str = "GOOSE_ELICITATION_SIGNAL_TEST";

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_for(output: &mut impl ReadFd, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut received = String::new();
    while !received.contains(expected) {
        assert!(
            Instant::now() < deadline,
            "waiting for {expected:?}, got {received:?}"
        );
        let mut descriptor = libc::pollfd {
            fd: output.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // The descriptor is owned by output and remains open throughout poll.
        let ready = unsafe { libc::poll(&mut descriptor, 1, 50) };
        assert!(ready >= 0);
        if ready > 0 {
            let mut byte = [0];
            assert_eq!(
                output.read(&mut byte).unwrap(),
                1,
                "child closed output: {received}"
            );
            received.push(char::from(byte[0]));
        }
    }
}

trait ReadFd: Read + AsRawFd {}
impl<T: Read + AsRawFd> ReadFd for T {}

fn own_descriptor(fd: libc::c_int) -> File {
    // Only descriptors returned by successful openpty/pipe calls reach this helper.
    let file = unsafe { File::from_raw_fd(fd) };
    assert_eq!(
        unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) },
        0
    );
    file
}

#[test_case(false, false, Some(false); "pipe sigint")]
#[test_case(true, false, Some(false); "pty sigint")]
#[test_case(false, true, Some(false); "pipe partial and earlier field")]
#[test_case(true, true, Some(false); "pty partial and earlier field")]
#[test_case(true, true, Some(true); "pty keyboard ctrl c")]
#[test_case(false, false, None; "pipe completed lines")]
#[test_case(true, false, None; "pty completed lines")]
fn freeform_input_preserves_cancellation_and_ownership(
    terminal: bool,
    partial: bool,
    keyboard: Option<bool>,
) {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "session::elicitation::signal_tests::signal_child",
            "--nocapture",
        ])
        .env(
            CHILD_MODE,
            if keyboard.is_none() {
                "complete"
            } else if partial {
                "partial"
            } else {
                "empty"
            },
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    let mut pipe_input = None;
    let input = if terminal {
        let mut master = -1;
        let mut slave = -1;
        // openpty initializes both descriptors; File takes ownership after success.
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            },
            0
        );
        let master = own_descriptor(master);
        let slave = own_descriptor(slave);
        command.stdin(Stdio::from(slave));
        // Only async-signal-safe syscalls run between fork and exec.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                    return Err(io::Error::last_os_error());
                }
                // Closing the test's PTY master must not kill the child during shutdown.
                libc::signal(libc::SIGHUP, libc::SIG_IGN);
                Ok(())
            });
        }
        master
    } else {
        let mut descriptors = [-1; 2];
        // Keep a read descriptor to observe when the child consumed the partial line.
        assert_eq!(unsafe { libc::pipe(descriptors.as_mut_ptr()) }, 0);
        let reader = own_descriptor(descriptors[0]);
        let writer = own_descriptor(descriptors[1]);
        command.stdin(Stdio::from(reader.try_clone().unwrap()));
        pipe_input = Some(reader);
        writer
    };

    let mut child = ChildGuard(command.spawn().unwrap());
    // Close the PTY master before waiting for the child, including on assertion failure.
    let mut input = input;
    let mut output = child.0.stdout.take().unwrap();
    if keyboard.is_none() {
        wait_for(&mut output, "PREFETCH_INPUT");
        input.write_all("\ncafé\nnext chat\n".as_bytes()).unwrap();
        wait_for(&mut output, "ACCEPTED_COMPLETE_ANSWERS");
    } else {
        if partial {
            wait_for(&mut output, "a_previous: ");
            input.write_all(b"prior private answer\n").unwrap();
        }
        wait_for(&mut output, "z_answer: ");
        if partial {
            input.write_all(b"unfinished").unwrap();
            if let Some(pipe) = pipe_input {
                let deadline = Instant::now() + Duration::from_secs(5);
                loop {
                    let mut pending: libc::c_int = 0;
                    assert_eq!(
                        unsafe { libc::ioctl(pipe.as_raw_fd(), libc::FIONREAD as _, &mut pending) },
                        0
                    );
                    if pending == 0 {
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "child did not consume partial input"
                    );
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
        if keyboard == Some(true) {
            input.write_all(&[3]).unwrap();
        } else {
            // Signal only the isolated child, never the test runner's process group.
            assert_eq!(
                unsafe { libc::kill(child.0.id() as libc::pid_t, libc::SIGINT) },
                0
            );
        }
        wait_for(&mut output, "CANCELLED_WITHOUT_ANSWERS");
        input.write_all(b"next chat\n").unwrap();
    }
    wait_for(&mut output, "NEXT_INPUT_PRESERVED");
    drop(input);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(
            Instant::now() < deadline,
            "child did not exit after cancellation"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn signal_child() {
    let Ok(mode) = std::env::var(CHILD_MODE) else {
        return;
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let token = CancellationToken::new();
        let cancelled = token.clone();
        let mut signal = Box::pin(tokio::signal::ctrl_c());
        poll_fn(|cx| {
            assert!(signal.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
        tokio::spawn(async move {
            signal.await.unwrap();
            cancelled.cancel();
        });
        let properties = if mode == "complete" {
            serde_json::json!({"a_previous": {"type": "string", "default": "default"}, "z_answer": {"type": "string"}})
        } else if mode == "partial" {
            serde_json::json!({"a_previous": {"type": "string"}, "z_answer": {"type": "string"}})
        } else {
            serde_json::json!({"z_answer": {"type": "string"}})
        };
        let schema = serde_json::json!({"type": "object", "properties": properties});
        if mode == "complete" {
            println!("PREFETCH_INPUT");
            io::stdout().flush().unwrap();
            assert!(!io::stdin().lock().fill_buf().unwrap().is_empty());
        }
        let flags = unsafe { libc::fcntl(0, libc::F_GETFL) };
        let result = collect_elicitation_input("", &schema, &token).unwrap();
        assert_eq!(unsafe { libc::fcntl(0, libc::F_GETFL) }, flags);
        if mode == "complete" {
            assert_eq!(result.action, ElicitationAction::Accept);
            assert_eq!(result.user_data["a_previous"], "default");
            assert_eq!(result.user_data["z_answer"], "café");
            println!("ACCEPTED_COMPLETE_ANSWERS");
        } else {
            assert_eq!(result.action, ElicitationAction::Cancel);
            assert!(result.user_data.is_empty());
            println!("CANCELLED_WITHOUT_ANSWERS");
        }
        io::stdout().flush().unwrap();
        assert_eq!(
            read_line_from(&mut io::stdin().lock()).unwrap().as_deref(),
            Some("next chat")
        );
        println!("NEXT_INPUT_PRESERVED");
    });
}
