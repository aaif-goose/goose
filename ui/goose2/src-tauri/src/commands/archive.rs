use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::Serialize;
use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZipArchiveResult {
    pub data: String,
    pub filename: String,
}

fn validate_archive_root(source_path: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(source_path);
    if path.as_os_str().is_empty() {
        return Err("Selected directory path is empty".to_string());
    }

    let metadata = fs::metadata(&path)
        .map_err(|err| format!("Failed to access directory '{}': {}", path.display(), err))?;
    if !metadata.is_dir() {
        return Err(format!(
            "Selected path '{}' is not a directory",
            path.display()
        ));
    }

    if !path.join("SKILL.md").is_file() {
        return Err(format!(
            "Selected directory '{}' must contain a SKILL.md file",
            path.display()
        ));
    }

    Ok(path)
}

fn archive_root_name(root: &Path) -> String {
    root.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("skill")
        .to_string()
}

fn archive_filename(root: &Path) -> String {
    format!("{}.skill.zip", archive_root_name(root))
}

fn archive_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn relative_zip_path(root: &Path, path: &Path) -> Result<String, String> {
    let relative = path.strip_prefix(root).map_err(|err| {
        format!(
            "Failed to resolve '{}' relative to '{}': {}",
            path.display(),
            root.display(),
            err
        )
    })?;
    Ok(format!(
        "{}/{}",
        archive_root_name(root),
        archive_path(relative)
    ))
}

fn should_skip_archive_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(".git") | Some(".hg") | Some(".svn")
    )
}

fn collect_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir)
            .map_err(|err| format!("Failed to read directory '{}': {}", dir.display(), err))?;
        for entry in entries {
            let entry = entry.map_err(|err| {
                format!(
                    "Failed to read directory entry in '{}': {}",
                    dir.display(),
                    err
                )
            })?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|err| format!("Failed to inspect '{}': {}", path.display(), err))?;
            if file_type.is_dir() {
                if !should_skip_archive_dir(&path) {
                    stack.push(path);
                }
            } else if file_type.is_file() {
                files.push(path);
            }
        }
    }

    files.sort_by_key(|path| path.to_string_lossy().to_lowercase());
    Ok(files)
}

fn zip_directory(root: &Path) -> Result<Vec<u8>, String> {
    let mut cursor = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut cursor);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    for path in collect_files(root)? {
        let name = relative_zip_path(root, &path)?;
        zip.start_file(name, options)
            .map_err(|err| format!("Failed to add file to archive: {err}"))?;
        let bytes = fs::read(&path)
            .map_err(|err| format!("Failed to read '{}': {}", path.display(), err))?;
        zip.write_all(&bytes)
            .map_err(|err| format!("Failed to write archive entry: {err}"))?;
    }

    zip.finish()
        .map_err(|err| format!("Failed to finish archive: {err}"))?;
    Ok(cursor.into_inner())
}

#[tauri::command]
pub async fn create_zip_archive(source_path: String) -> Result<ZipArchiveResult, String> {
    tokio::task::spawn_blocking(move || {
        let root = validate_archive_root(&source_path)?;
        let bytes = zip_directory(&root)?;
        Ok(ZipArchiveResult {
            data: BASE64.encode(bytes),
            filename: archive_filename(&root),
        })
    })
    .await
    .map_err(|err| format!("Failed to create archive: {err}"))?
}

#[cfg(test)]
mod tests {
    use super::{archive_filename, validate_archive_root, zip_directory};
    use std::fs;
    use tempfile::tempdir;
    use zip::ZipArchive;

    #[test]
    fn validates_skill_directory() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("SKILL.md"), "---\nname: test\n---\n").expect("skill");

        assert_eq!(
            validate_archive_root(dir.path().to_str().unwrap()).unwrap(),
            dir.path()
        );
    }

    #[test]
    fn rejects_directory_without_skill_file() {
        let dir = tempdir().expect("tempdir");

        let error = validate_archive_root(dir.path().to_str().unwrap()).expect_err("missing skill");

        assert!(error.contains("SKILL.md"));
    }

    #[test]
    fn creates_archive_with_root_directory() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path().join("my-skill");
        fs::create_dir_all(root.join("assets")).expect("dirs");
        fs::write(root.join("SKILL.md"), "---\nname: my-skill\n---\nBody").expect("skill");
        fs::write(root.join("assets").join("note.txt"), "note").expect("note");

        let bytes = zip_directory(&root).expect("zip");
        let mut archive = ZipArchive::new(std::io::Cursor::new(bytes)).expect("archive");

        assert!(archive.by_name("my-skill/SKILL.md").is_ok());
        assert!(archive.by_name("my-skill/assets/note.txt").is_ok());
        assert_eq!(archive_filename(&root), "my-skill.skill.zip");
    }
}
