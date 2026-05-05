use super::{
    build_file_tree_entry, inspect_attachment_path, inspect_attachment_paths,
    normalize_attachment_paths, normalize_roots, read_directory_entries, read_image_attachment,
    scan_files_for_mentions, validate_shareable_source_file, MAX_IMAGE_ATTACHMENT_BYTES,
};
use base64::Engine;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use tempfile::tempdir;

fn git_tempdir() -> tempfile::TempDir {
    let dir = tempdir().expect("tempdir");
    Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(dir.path())
        .output()
        .expect("git init");
    dir
}

#[test]
fn respects_gitignore() {
    let dir = git_tempdir();
    let root = dir.path();
    let src = root.join("src");
    let ignored = root.join("node_modules").join("pkg");

    fs::create_dir_all(&src).expect("src dir");
    fs::create_dir_all(&ignored).expect("ignored dir");
    fs::write(src.join("main.ts"), "export {}").expect("source file");
    fs::write(ignored.join("index.js"), "module.exports = {}").expect("ignored file");
    fs::write(root.join(".gitignore"), "node_modules/\n").expect(".gitignore");

    let files = scan_files_for_mentions(vec![root.to_string_lossy().to_string()], Some(50));

    let joined = files.join("\n");
    assert!(joined.contains("main.ts"), "should include source files");
    assert!(
        !joined.contains("node_modules"),
        "should respect .gitignore"
    );
}

#[test]
fn skips_hidden_files() {
    let dir = git_tempdir();
    let root = dir.path();

    fs::write(root.join("visible.ts"), "").expect("visible file");
    fs::write(root.join(".hidden"), "").expect("hidden file");

    let files = scan_files_for_mentions(vec![root.to_string_lossy().to_string()], Some(50));

    let joined = files.join("\n");
    assert!(joined.contains("visible.ts"));
    assert!(!joined.contains(".hidden"));
}

#[test]
fn lists_directory_entries_with_expected_sorting_and_visibility() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    fs::create_dir_all(root.join(".git")).expect(".git dir");
    fs::create_dir_all(root.join(".github")).expect(".github dir");
    fs::create_dir_all(root.join("node_modules")).expect("node_modules dir");
    fs::create_dir_all(root.join("src")).expect("src dir");
    fs::write(root.join(".env"), "").expect(".env");
    fs::write(root.join(".gitignore"), "node_modules/\n").expect(".gitignore");
    fs::write(root.join("README.md"), "").expect("README");
    fs::write(root.join("alpha.ts"), "").expect("alpha");

    let entries = read_directory_entries(root).expect("entries");

    assert_eq!(
        entries,
        vec![
            super::FileTreeEntry {
                name: ".github".into(),
                path: root.join(".github").to_string_lossy().into_owned(),
                kind: "directory".into(),
            },
            super::FileTreeEntry {
                name: "node_modules".into(),
                path: root.join("node_modules").to_string_lossy().into_owned(),
                kind: "directory".into(),
            },
            super::FileTreeEntry {
                name: "src".into(),
                path: root.join("src").to_string_lossy().into_owned(),
                kind: "directory".into(),
            },
            super::FileTreeEntry {
                name: ".env".into(),
                path: root.join(".env").to_string_lossy().into_owned(),
                kind: "file".into(),
            },
            super::FileTreeEntry {
                name: ".gitignore".into(),
                path: root.join(".gitignore").to_string_lossy().into_owned(),
                kind: "file".into(),
            },
            super::FileTreeEntry {
                name: "alpha.ts".into(),
                path: root.join("alpha.ts").to_string_lossy().into_owned(),
                kind: "file".into(),
            },
            super::FileTreeEntry {
                name: "README.md".into(),
                path: root.join("README.md").to_string_lossy().into_owned(),
                kind: "file".into(),
            },
        ]
    );
}

#[test]
fn list_directory_entries_errors_for_missing_paths() {
    let dir = tempdir().expect("tempdir");
    let missing = dir.path().join("missing");

    let error = read_directory_entries(&missing).expect_err("missing dir should error");
    assert!(error.contains("Directory does not exist"));
}

#[test]
fn build_file_tree_entry_skips_missing_children() {
    let dir = tempdir().expect("tempdir");
    let missing = dir.path().join("missing.ts");

    let entry = build_file_tree_entry(missing, "missing.ts".into());

    assert_eq!(entry, None);
}

#[test]
#[cfg(unix)]
fn list_directory_entries_errors_for_unreadable_directories() {
    let dir = tempdir().expect("tempdir");
    let blocked = dir.path().join("blocked");
    fs::create_dir(&blocked).expect("blocked dir");

    let original_permissions = fs::metadata(&blocked).expect("metadata").permissions();
    let mut unreadable_permissions = original_permissions.clone();
    unreadable_permissions.set_mode(0o000);
    fs::set_permissions(&blocked, unreadable_permissions).expect("set unreadable");

    let error = read_directory_entries(&blocked).expect_err("unreadable dir should error");

    let mut restored_permissions = original_permissions;
    restored_permissions.set_mode(0o700);
    fs::set_permissions(&blocked, restored_permissions).expect("restore permissions");

    assert!(error.contains("Failed to read directory"));
}

#[test]
fn inspects_file_and_directory_attachments() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    let folder = root.join("screenshots");
    let file = root.join("report.txt");

    fs::create_dir_all(&folder).expect("folder");
    fs::write(&file, "hello").expect("file");

    let inspected_dir = inspect_attachment_path(&folder).expect("directory");
    let inspected_file = inspect_attachment_path(&file).expect("file");

    assert_eq!(inspected_dir.kind, "directory");
    assert_eq!(inspected_dir.name, "screenshots");
    assert_eq!(inspected_dir.mime_type, None);

    assert_eq!(inspected_file.kind, "file");
    assert_eq!(inspected_file.name, "report.txt");
    assert_eq!(inspected_file.mime_type.as_deref(), Some("text/plain"));
}

#[test]
fn reads_image_attachment_payloads() {
    let dir = tempdir().expect("tempdir");
    let image = dir.path().join("pixel.png");
    let png_bytes = base64::engine::general_purpose::STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9sU4nS0AAAAASUVORK5CYII=")
        .expect("decode png");

    fs::write(&image, png_bytes).expect("png file");

    let payload = read_image_attachment(image.to_string_lossy().into_owned()).expect("payload");

    assert_eq!(payload.mime_type, "image/png");
    assert!(!payload.base64.is_empty());
}

#[test]
fn dedupes_attachment_paths_using_platform_path_rules() {
    let normalized = normalize_attachment_paths(vec![
        "/tmp/Readme.md".into(),
        "/tmp/README.md".into(),
        "/tmp/Readme.md".into(),
    ]);

    if cfg!(any(target_os = "macos", target_os = "windows")) {
        assert_eq!(normalized, vec![PathBuf::from("/tmp/Readme.md")]);
    } else {
        assert_eq!(
            normalized,
            vec![
                PathBuf::from("/tmp/Readme.md"),
                PathBuf::from("/tmp/README.md")
            ]
        );
    }
}

#[test]
fn skips_invalid_attachment_paths_without_dropping_valid_ones() {
    let dir = tempdir().expect("tempdir");
    let valid = dir.path().join("report.txt");
    let missing = dir.path().join("missing.txt");
    fs::write(&valid, "hello").expect("file");

    let attachments = inspect_attachment_paths(vec![
        valid.to_string_lossy().into_owned(),
        missing.to_string_lossy().into_owned(),
    ])
    .expect("attachments");

    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments[0].name, "report.txt");
    assert_eq!(attachments[0].kind, "file");
}

#[test]
fn dedupes_mention_roots_using_platform_path_rules() {
    let normalized = normalize_roots(vec![
        "/tmp/Workspace".into(),
        "/tmp/workspace".into(),
        "/tmp/Workspace".into(),
    ]);

    if cfg!(any(target_os = "macos", target_os = "windows")) {
        assert_eq!(normalized, vec![PathBuf::from("/tmp/Workspace")]);
    } else {
        assert_eq!(
            normalized,
            vec![
                PathBuf::from("/tmp/Workspace"),
                PathBuf::from("/tmp/workspace")
            ]
        );
    }
}

#[test]
fn rejects_oversized_image_attachment_payloads() {
    let dir = tempdir().expect("tempdir");
    let image = dir.path().join("huge.png");
    fs::write(
        &image,
        vec![0_u8; (MAX_IMAGE_ATTACHMENT_BYTES as usize) + 1],
    )
    .expect("oversized image file");

    let error =
        read_image_attachment(image.to_string_lossy().into_owned()).expect_err("size limit");

    assert!(error.contains("exceeds the 20 MB limit"));
}

#[test]
fn shareable_source_validation_accepts_agent_and_skill_files() {
    let dir = tempdir().expect("tempdir");
    let agent_file = dir.path().join(".agents").join("agents").join("helper.md");
    let skill_file = dir
        .path()
        .join(".agents")
        .join("skills")
        .join("review")
        .join("SKILL.md");
    let config_skill_file = dir
        .path()
        .join("Block")
        .join("goose")
        .join("skills")
        .join("legacy")
        .join("SKILL.md");
    let plugin_skill_file = dir
        .path()
        .join("Block")
        .join("goose")
        .join("plugins")
        .join("plugin-a")
        .join("skills")
        .join("plugin-review")
        .join("SKILL.md");
    fs::create_dir_all(agent_file.parent().expect("agent parent")).expect("agent dir");
    fs::create_dir_all(skill_file.parent().expect("skill parent")).expect("skill dir");
    fs::create_dir_all(config_skill_file.parent().expect("config skill parent"))
        .expect("config skill dir");
    fs::create_dir_all(plugin_skill_file.parent().expect("plugin skill parent"))
        .expect("plugin skill dir");
    fs::write(&agent_file, "---\nname: Helper\n---\n").expect("agent file");
    fs::write(&skill_file, "---\nname: review\n---\n").expect("skill file");
    fs::write(&config_skill_file, "---\nname: legacy\n---\n").expect("config skill file");
    fs::write(&plugin_skill_file, "---\nname: plugin-review\n---\n").expect("plugin skill file");

    assert!(validate_shareable_source_file(&agent_file).is_ok());
    assert!(validate_shareable_source_file(&skill_file).is_ok());
    assert!(validate_shareable_source_file(&config_skill_file).is_ok());
    assert!(validate_shareable_source_file(&plugin_skill_file).is_ok());
}

#[test]
fn shareable_source_validation_rejects_unrelated_files() {
    let dir = tempdir().expect("tempdir");
    let markdown_file = dir.path().join("notes.md");
    let text_file = dir
        .path()
        .join(".agents")
        .join("skills")
        .join("review")
        .join("notes.txt");
    fs::create_dir_all(text_file.parent().expect("text parent")).expect("skill dir");
    fs::write(&markdown_file, "# private notes\n").expect("markdown file");
    fs::write(&text_file, "private notes\n").expect("text file");

    assert!(validate_shareable_source_file(&markdown_file).is_err());
    assert!(validate_shareable_source_file(&text_file).is_err());
}

#[test]
#[cfg(unix)]
fn shareable_source_validation_rejects_symlinks_outside_allowed_roots() {
    let dir = tempdir().expect("tempdir");
    let private_file = dir.path().join("private.md");
    let linked_agent = dir.path().join(".agents").join("agents").join("linked.md");
    fs::create_dir_all(linked_agent.parent().expect("agent parent")).expect("agent dir");
    fs::write(&private_file, "private notes\n").expect("private file");
    std::os::unix::fs::symlink(&private_file, &linked_agent).expect("symlink");

    assert!(validate_shareable_source_file(&linked_agent).is_err());
}
