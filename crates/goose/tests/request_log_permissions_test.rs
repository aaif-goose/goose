#![cfg(unix)]

use goose::providers::utils::RequestLog;
use goose_providers::request_log::RequestLogger;
use serial_test::serial;
use std::fs;
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::path::Path;

const LOGS_TO_KEEP: usize = 10;

struct Umask(libc::mode_t);

impl Umask {
    fn set(mask: libc::mode_t) -> Self {
        Self(unsafe { libc::umask(mask) })
    }
}

impl Drop for Umask {
    fn drop(&mut self) {
        unsafe { libc::umask(self.0) };
    }
}

fn mode(path: &Path) -> u32 {
    fs::symlink_metadata(path).unwrap().mode() & 0o777
}

fn chmod(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

fn write_request(log: &RequestLog, text: &str) {
    let mut handle = log.start().unwrap();
    handle.write(text).unwrap();
}

fn request_logs(logs_dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(logs_dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.starts_with("llm_request."))
        .collect();
    names.sort();
    names
}

fn with_path_root(test: impl FnOnce(&Path)) {
    let root = tempfile::tempdir().unwrap();
    let _env = env_lock::lock_env([("GOOSE_PATH_ROOT", Some(root.path().to_str().unwrap()))]);
    test(root.path());
}

#[test]
#[serial]
fn new_and_rotated_request_logs_are_owner_only_under_any_umask() {
    for mask in [0o022, 0o000, 0o400] {
        with_path_root(|root| {
            let _umask = Umask::set(mask);
            let log = RequestLog::new(LOGS_TO_KEEP).unwrap();
            for i in 0..LOGS_TO_KEEP + 2 {
                write_request(&log, &format!("request {i}"));
            }

            let logs_dir = root.join("state/logs");
            assert_eq!(mode(&logs_dir), 0o700, "umask {mask:o}");
            let names = request_logs(&logs_dir);
            assert_eq!(names.len(), LOGS_TO_KEEP, "umask {mask:o}");
            for name in names {
                assert_eq!(mode(&logs_dir.join(&name)), 0o600, "{name}, umask {mask:o}");
            }
            assert_eq!(
                mode(&root.join("state")),
                0o777 & !mask,
                "the shared parent keeps its mode, umask {mask:o}"
            );
        });
    }
}

#[test]
#[serial]
fn permissive_logs_from_older_versions_are_tightened_and_kept() {
    with_path_root(|root| {
        let state_dir = root.join("state");
        let logs_dir = state_dir.join("logs");
        fs::create_dir_all(&logs_dir).unwrap();
        chmod(&state_dir, 0o755);
        chmod(&logs_dir, 0o755);
        let stale = "llm_request.4f1c2a9e-0000-4000-8000-000000000000.jsonl";
        for (name, text) in [
            ("llm_request.0.jsonl", "old 0"),
            ("llm_request.1.jsonl", "old 1"),
            (stale, "interrupted"),
            ("notes.txt", "not a request log"),
        ] {
            fs::write(logs_dir.join(name), text).unwrap();
            chmod(&logs_dir.join(name), 0o644);
        }

        let log = RequestLog::new(LOGS_TO_KEEP).unwrap();
        write_request(&log, "new");

        assert_eq!(mode(&logs_dir), 0o700);
        for name in request_logs(&logs_dir) {
            assert_eq!(mode(&logs_dir.join(&name)), 0o600, "{name}");
        }
        assert_eq!(
            fs::read_to_string(logs_dir.join("llm_request.1.jsonl")).unwrap(),
            "old 0"
        );
        assert_eq!(
            fs::read_to_string(logs_dir.join("llm_request.2.jsonl")).unwrap(),
            "old 1"
        );
        assert_eq!(
            fs::read_to_string(logs_dir.join(stale)).unwrap(),
            "interrupted"
        );
        assert_eq!(mode(&logs_dir.join("notes.txt")), 0o644);
        assert_eq!(mode(&state_dir), 0o755);
    });
}

#[test]
#[serial]
fn logs_without_owner_read_permission_are_tightened_and_kept() {
    with_path_root(|root| {
        let logs_dir = root.join("state/logs");
        fs::create_dir_all(&logs_dir).unwrap();
        fs::write(logs_dir.join("llm_request.0.jsonl"), "write only").unwrap();
        chmod(&logs_dir.join("llm_request.0.jsonl"), 0o200);
        chmod(&logs_dir, 0o300);

        let log = RequestLog::new(LOGS_TO_KEEP).unwrap();
        write_request(&log, "new");

        assert_eq!(mode(&logs_dir), 0o700);
        for name in request_logs(&logs_dir) {
            assert_eq!(mode(&logs_dir.join(&name)), 0o600, "{name}");
        }
        assert_eq!(
            fs::read_to_string(logs_dir.join("llm_request.1.jsonl")).unwrap(),
            "write only"
        );
    });
}

#[test]
#[serial]
fn logs_without_any_owner_permission_are_tightened_and_kept() {
    with_path_root(|root| {
        let log = RequestLog::new(LOGS_TO_KEEP).unwrap();
        let logs_dir = root.join("state/logs");
        for (name, text, mode) in [
            ("llm_request.0.jsonl", "no access", 0o000),
            ("llm_request.1.jsonl", "others read and write", 0o066),
        ] {
            fs::write(logs_dir.join(name), text).unwrap();
            chmod(&logs_dir.join(name), mode);
        }

        write_request(&log, "new");

        for name in request_logs(&logs_dir) {
            assert_eq!(mode(&logs_dir.join(&name)), 0o600, "{name}");
        }
        assert_eq!(
            fs::read_to_string(logs_dir.join("llm_request.1.jsonl")).unwrap(),
            "no access"
        );
        assert_eq!(
            fs::read_to_string(logs_dir.join("llm_request.2.jsonl")).unwrap(),
            "others read and write"
        );
    });
}

#[test]
#[serial]
fn symlinked_log_directory_is_refused_and_its_target_is_untouched() {
    with_path_root(|root| {
        let outside = tempfile::tempdir().unwrap();
        chmod(outside.path(), 0o755);
        fs::create_dir_all(root.join("state")).unwrap();
        symlink(outside.path(), root.join("state/logs")).unwrap();

        let error = RequestLog::new(LOGS_TO_KEEP).err().unwrap();

        assert!(error.to_string().contains("symbolic link"), "{error}");
        assert_eq!(mode(outside.path()), 0o755);
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    });
}

#[test]
#[serial]
fn symlinked_request_log_is_refused_and_its_target_is_untouched() {
    with_path_root(|root| {
        let log = RequestLog::new(LOGS_TO_KEEP).unwrap();
        let outside = root.join("outside.txt");
        fs::write(&outside, "secret").unwrap();
        chmod(&outside, 0o644);
        symlink(&outside, root.join("state/logs/llm_request.3.jsonl")).unwrap();

        let error = log.start().err().unwrap();

        assert!(error.to_string().contains("symbolic link"), "{error}");
        assert_eq!(mode(&outside), 0o644);
        assert_eq!(fs::read_to_string(&outside).unwrap(), "secret");
    });
}

#[test]
#[serial]
fn hard_linked_request_log_is_refused_and_left_unchanged() {
    with_path_root(|root| {
        let log = RequestLog::new(LOGS_TO_KEEP).unwrap();
        let outside = root.join("outside.txt");
        fs::write(&outside, "secret").unwrap();
        chmod(&outside, 0o644);
        fs::hard_link(&outside, root.join("state/logs/llm_request.2.jsonl")).unwrap();

        let error = log.start().err().unwrap();

        assert!(error.to_string().contains("hard links"), "{error}");
        assert_eq!(mode(&outside), 0o644);
        assert_eq!(fs::metadata(&outside).unwrap().nlink(), 2);
        assert_eq!(fs::read_to_string(&outside).unwrap(), "secret");
    });
}
