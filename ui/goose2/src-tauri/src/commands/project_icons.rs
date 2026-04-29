use base64::{engine::general_purpose, Engine as _};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_ICON_CANDIDATES: usize = 18;
const MAX_PROJECT_ICON_BYTES: u64 = 512 * 1024;

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProjectIconCandidate {
    pub id: String,
    pub label: String,
    pub icon: String,
    pub source_dir: String,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProjectIconData {
    pub icon: String,
}

struct ScoredProjectIconPath {
    score: i32,
    path: PathBuf,
    path_string: String,
    label: String,
    source_dir: String,
    group_key: String,
}

fn is_project_icon_extension(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("svg" | "png" | "ico" | "jpg" | "jpeg" | "webp")
    )
}

fn is_ignored_icon_search_dir(root: &Path, path: &Path) -> bool {
    let relative_parent = path
        .strip_prefix(root)
        .unwrap_or(path)
        .parent()
        .unwrap_or_else(|| Path::new(""));

    relative_parent.components().any(|component| {
        let name = component.as_os_str().to_string_lossy().to_ascii_lowercase();
        matches!(
            name.as_str(),
            "node_modules" | "target" | "dist" | "build" | ".git" | ".next" | ".turbo"
        )
    })
}

fn is_generated_icon_variant(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let normalized = file_name.to_ascii_lowercase();
    let stem = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mostly_size_token = stem
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, 'x' | '@' | '-' | '_'));

    normalized.starts_with("appicon-")
        || normalized.starts_with("square")
        || normalized.starts_with("storelogo")
        || normalized.contains("template")
        || normalized.contains("@2x")
        || normalized.contains("@3x")
        || mostly_size_token
        || stem
            .strip_prefix("icon-")
            .is_some_and(|suffix| suffix.chars().all(|c| c.is_ascii_digit()))
        || stem
            .strip_prefix("icon@")
            .is_some_and(|suffix| suffix.ends_with('x'))
}

fn is_likely_project_icon(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let normalized = file_name.to_ascii_lowercase();
    normalized == "favicon.ico"
        || normalized == "favicon.svg"
        || normalized == "favicon.png"
        || normalized.starts_with("apple-touch-icon")
        || normalized.starts_with("mstile-")
        || normalized.contains("logo")
        || normalized.contains("brand")
        || normalized.contains("wordmark")
        || normalized.contains("app-icon")
        || normalized.contains("appicon")
        || normalized.contains("icon")
}

fn project_icon_score(root: &Path, path: &Path) -> i32 {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let relative = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_ascii_lowercase();

    let mut score = 100;
    if file_name.starts_with("favicon") {
        score -= 35;
    }
    if file_name.contains("logo") {
        score -= 30;
    }
    if file_name.contains("brand") || file_name.contains("wordmark") {
        score -= 25;
    }
    if relative.starts_with("public/")
        || relative.starts_with("static/")
        || relative.starts_with("assets/")
        || relative.starts_with("src/assets/")
        || relative.starts_with("src/images/")
    {
        score -= 20;
    }
    score + relative.matches('/').count() as i32
}

fn project_icon_group_key(path: &Path) -> String {
    let file_stem = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let normalized = file_stem
        .replace("goose-logo", "logo")
        .replace("logo-codename-goose", "logo")
        .replace("codename-goose", "logo");

    if normalized.contains("favicon") {
        "favicon".to_string()
    } else if normalized.contains("wordmark") {
        "wordmark".to_string()
    } else if normalized.contains("brand") {
        "brand".to_string()
    } else if normalized.contains("logo") {
        "logo".to_string()
    } else if normalized.contains("app-icon") || normalized.contains("appicon") {
        "app-icon".to_string()
    } else {
        normalized
    }
}

fn project_icon_root_key(root: &Path) -> String {
    root.to_string_lossy().into_owned()
}

fn project_icon_candidate_group_key(root: &Path, path: &Path) -> String {
    format!(
        "{}:{}",
        project_icon_root_key(root),
        project_icon_group_key(path)
    )
}

fn read_project_icon_data_url(path: &Path) -> Result<String, String> {
    let metadata = fs::metadata(path).map_err(|e| format!("Failed to inspect icon: {}", e))?;
    if !metadata.is_file() {
        return Err("Icon path is not a file".to_string());
    }
    if metadata.len() > MAX_PROJECT_ICON_BYTES {
        return Err("Icon file is too large".to_string());
    }

    let mime = mime_guess::from_path(path)
        .first_or_octet_stream()
        .essence_str()
        .to_string();
    if !matches!(
        mime.as_str(),
        "image/svg+xml"
            | "image/png"
            | "image/x-icon"
            | "image/vnd.microsoft.icon"
            | "image/jpeg"
            | "image/webp"
    ) {
        return Err("Icon file type is not supported".to_string());
    }

    let bytes = fs::read(path).map_err(|e| format!("Failed to read icon: {}", e))?;
    Ok(format!(
        "data:{};base64,{}",
        mime,
        general_purpose::STANDARD.encode(bytes)
    ))
}

#[tauri::command]
pub fn scan_project_icons(working_dirs: Vec<String>) -> Result<Vec<ProjectIconCandidate>, String> {
    let mut candidates: Vec<ScoredProjectIconPath> = Vec::new();
    let mut seen = HashSet::new();

    for dir in working_dirs {
        let root = PathBuf::from(dir.trim());
        if !root.is_dir() {
            continue;
        }

        let source_dir = root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("project")
            .to_string();

        let walker = ignore::WalkBuilder::new(&root)
            .max_depth(Some(6))
            .standard_filters(true)
            .build();

        for entry in walker.flatten() {
            let path = entry.path();
            if !path.is_file()
                || is_ignored_icon_search_dir(&root, path)
                || is_generated_icon_variant(path)
                || !is_project_icon_extension(path)
                || !is_likely_project_icon(path)
            {
                continue;
            }

            let path_string = path.to_string_lossy().into_owned();
            if !seen.insert(path_string.clone()) {
                continue;
            }

            let relative = path.strip_prefix(&root).unwrap_or(path);
            let label = relative.to_string_lossy().into_owned();
            let score = project_icon_score(&root, path);
            let group_key = project_icon_candidate_group_key(&root, path);
            candidates.push(ScoredProjectIconPath {
                score,
                path: path.to_path_buf(),
                path_string,
                label,
                source_dir: source_dir.clone(),
                group_key,
            });
        }
    }

    candidates.sort_by(|a, b| a.score.cmp(&b.score).then_with(|| a.label.cmp(&b.label)));

    let mut seen_groups = HashSet::new();
    let mut icons = Vec::new();
    for candidate in candidates {
        if icons.len() >= MAX_ICON_CANDIDATES {
            break;
        }
        if seen_groups.contains(&candidate.group_key) {
            continue;
        }
        let icon = match read_project_icon_data_url(&candidate.path) {
            Ok(icon) => icon,
            Err(_) => continue,
        };
        seen_groups.insert(candidate.group_key);
        icons.push(ProjectIconCandidate {
            id: candidate.path_string.clone(),
            label: candidate.label,
            icon,
            source_dir: candidate.source_dir,
        });
    }

    Ok(icons)
}

#[tauri::command]
pub fn read_project_icon(path: String) -> Result<ProjectIconData, String> {
    let path = PathBuf::from(path.trim());
    if !is_project_icon_extension(&path) {
        return Err("Icon file type is not supported".to_string());
    }
    let icon = read_project_icon_data_url(&path)?;
    Ok(ProjectIconData { icon })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignored_icon_search_dirs_do_not_match_root_ancestors() {
        let root = Path::new("/Users/alice/build/myapp");
        let icon = root.join("public/logo.svg");

        assert!(!is_ignored_icon_search_dir(root, &icon));
    }

    #[test]
    fn ignored_icon_search_dirs_match_descendant_dirs() {
        let root = Path::new("/Users/alice/projects/myapp");
        let icon = root.join("dist/logo.svg");

        assert!(is_ignored_icon_search_dir(root, &icon));
    }

    #[test]
    fn project_icon_group_keys_distinguish_roots_with_same_basename() {
        let first_root = Path::new("/work/client");
        let second_root = Path::new("/archive/client");
        let first_icon = first_root.join("public/logo.svg");
        let second_icon = second_root.join("public/logo.svg");

        assert_ne!(
            project_icon_candidate_group_key(first_root, &first_icon),
            project_icon_candidate_group_key(second_root, &second_icon)
        );
    }
}
