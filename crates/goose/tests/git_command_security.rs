use goose::subprocess::git_command;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn run_git(cwd: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap()
}

fn assert_git_succeeded(output: &Output) {
    assert!(
        output.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn rejects_implicitly_discovered_bare_repository() {
    let temp_dir = tempfile::tempdir().unwrap();
    assert_git_succeeded(&run_git(
        temp_dir.path(),
        &["init", "--bare", "embedded.git"],
    ));

    let nested_dir = temp_dir.path().join("embedded.git/nested");
    fs::create_dir(&nested_dir).unwrap();

    assert_git_succeeded(&run_git(
        &nested_dir,
        &["-c", "safe.bareRepository=all", "rev-parse", "--git-dir"],
    ));

    let output = git_command()
        .args(["rev-parse", "--git-dir"])
        .current_dir(&nested_dir)
        .output()
        .unwrap();

    assert!(!output.status.success(), "implicit bare repo was accepted");
}

#[cfg(unix)]
#[test]
fn does_not_execute_repository_fsmonitor_hook() {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = tempfile::tempdir().unwrap();
    let repo_dir = temp_dir.path().join("repo");
    fs::create_dir(&repo_dir).unwrap();
    assert_git_succeeded(&run_git(&repo_dir, &["init"]));

    fs::write(repo_dir.join("tracked.txt"), "content").unwrap();
    assert_git_succeeded(&run_git(&repo_dir, &["add", "tracked.txt"]));

    let marker_path = temp_dir.path().join("fsmonitor-ran");
    let hook_path = temp_dir.path().join("fsmonitor-hook");
    fs::write(
        &hook_path,
        format!("#!/bin/sh\n: > '{}'\n", marker_path.display()),
    )
    .unwrap();
    fs::set_permissions(&hook_path, fs::Permissions::from_mode(0o755)).unwrap();
    assert_git_succeeded(&run_git(
        &repo_dir,
        &["config", "core.fsmonitor", hook_path.to_str().unwrap()],
    ));

    assert_git_succeeded(&run_git(&repo_dir, &["status", "--porcelain"]));
    assert!(marker_path.exists(), "fsmonitor hook fixture did not run");
    fs::remove_file(&marker_path).unwrap();

    let output = git_command()
        .args(["status", "--porcelain"])
        .current_dir(&repo_dir)
        .output()
        .unwrap();

    assert_git_succeeded(&output);
    assert!(!marker_path.exists(), "repository fsmonitor hook ran");
}

#[cfg(unix)]
#[test]
fn does_not_execute_repository_diff_external() {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = tempfile::tempdir().unwrap();
    let repo_dir = temp_dir.path().join("repo");
    fs::create_dir(&repo_dir).unwrap();
    assert_git_succeeded(&run_git(&repo_dir, &["init"]));

    fs::write(repo_dir.join("tracked.txt"), "initial\n").unwrap();
    assert_git_succeeded(&run_git(&repo_dir, &["add", "tracked.txt"]));
    assert_git_succeeded(&run_git(
        &repo_dir,
        &[
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "initial",
        ],
    ));

    fs::write(repo_dir.join("tracked.txt"), "modified\n").unwrap();

    let marker_path = temp_dir.path().join("diff-external-ran");
    let hook_path = temp_dir.path().join("diff-external-hook");
    fs::write(
        &hook_path,
        format!("#!/bin/sh\n: > '{}'\n", marker_path.display()),
    )
    .unwrap();
    fs::set_permissions(&hook_path, fs::Permissions::from_mode(0o755)).unwrap();
    assert_git_succeeded(&run_git(
        &repo_dir,
        &["config", "diff.external", hook_path.to_str().unwrap()],
    ));

    assert_git_succeeded(&run_git(&repo_dir, &["diff"]));
    assert!(
        marker_path.exists(),
        "diff.external hook fixture did not run"
    );
    fs::remove_file(&marker_path).unwrap();

    let output = git_command()
        .args(["diff", "--no-ext-diff"])
        .current_dir(&repo_dir)
        .output()
        .unwrap();

    assert_git_succeeded(&output);
    assert!(!marker_path.exists(), "repository diff.external hook ran");
}
