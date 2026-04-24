use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

fn projects_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    Ok(home.join(".goose").join("projects"))
}

fn generate_id() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut hasher = DefaultHasher::new();
    nanos.hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    let h1 = hasher.finish();
    // Hash again with a different seed for more bits
    h1.hash(&mut hasher);
    let h2 = hasher.finish();
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        (h1 >> 32) as u32,
        (h1 >> 16) as u16,
        h1 as u16,
        (h2 >> 48) as u16,
        h2 & 0xffffffffffff
    )
}

fn now_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    millis.to_string()
}

fn slugify(name: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "project".to_string()
    } else {
        slug
    }
}

/// Scan all project directories and find the one whose project.json has the given id.
/// Returns (dir_path, ProjectInfo).
fn find_project_by_id(id: &str) -> Result<(PathBuf, StoredProjectInfo), String> {
    let base = projects_dir()?;
    if !base.exists() {
        return Err(format!("Project with id \"{}\" not found", id));
    }

    let entries = fs::read_dir(&base).map_err(|e| format!("Failed to read projects dir: {}", e))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let project_json = path.join("project.json");
        if !project_json.exists() {
            continue;
        }
        let raw = match fs::read_to_string(&project_json) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let info: StoredProjectInfo = match serde_json::from_str(&raw) {
            Ok(i) => i,
            Err(_) => continue,
        };
        if info.id == id {
            return Ok((path, info));
        }
    }

    Err(format!("Project with id \"{}\" not found", id))
}

fn deserialize_working_dirs<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum WorkingDirsField {
        Many(Vec<String>),
        One(String),
        Null,
    }

    let value = Option::<WorkingDirsField>::deserialize(deserializer)?;
    let dirs = match value {
        Some(WorkingDirsField::Many(dirs)) => dirs,
        Some(WorkingDirsField::One(dir)) => vec![dir],
        Some(WorkingDirsField::Null) | None => Vec::new(),
    };

    Ok(dirs
        .into_iter()
        .map(|dir| dir.trim().to_string())
        .filter(|dir| !dir.is_empty())
        .collect())
}

fn project_artifacts_dir(project_dir: &Path) -> String {
    project_dir.join("artifacts").to_string_lossy().into_owned()
}

#[derive(serde::Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct StoredProjectInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub prompt: String,
    pub icon: String,
    pub color: String,
    pub preferred_provider: Option<String>,
    pub preferred_model: Option<String>,
    #[serde(
        default,
        alias = "workingDir",
        deserialize_with = "deserialize_working_dirs"
    )]
    pub working_dirs: Vec<String>,
    pub use_worktrees: bool,
    #[serde(default)]
    pub order: i32,
    #[serde(default)]
    pub archived_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub prompt: String,
    pub icon: String,
    pub color: String,
    pub preferred_provider: Option<String>,
    pub preferred_model: Option<String>,
    #[serde(default)]
    pub working_dirs: Vec<String>,
    pub use_worktrees: bool,
    #[serde(default)]
    pub order: i32,
    #[serde(default)]
    pub archived_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub artifacts_dir: String,
}

fn project_info_from_stored(project_dir: &Path, stored: StoredProjectInfo) -> ProjectInfo {
    ProjectInfo {
        id: stored.id,
        name: stored.name,
        description: stored.description,
        prompt: stored.prompt,
        icon: stored.icon,
        color: stored.color,
        preferred_provider: stored.preferred_provider,
        preferred_model: stored.preferred_model,
        working_dirs: stored.working_dirs,
        use_worktrees: stored.use_worktrees,
        order: stored.order,
        archived_at: stored.archived_at,
        created_at: stored.created_at,
        updated_at: stored.updated_at,
        artifacts_dir: project_artifacts_dir(project_dir),
    }
}

#[tauri::command]
pub fn list_projects() -> Result<Vec<ProjectInfo>, String> {
    let dir = projects_dir()?;

    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut projects = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|e| format!("Failed to read projects dir: {}", e))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let project_json = path.join("project.json");
        if !project_json.exists() {
            continue;
        }

        let raw = match fs::read_to_string(&project_json) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let info: StoredProjectInfo = match serde_json::from_str(&raw) {
            Ok(i) => i,
            Err(_) => continue,
        };

        projects.push(project_info_from_stored(&path, info));
    }

    projects.sort_by_key(|p| p.order);
    projects.retain(|p| p.archived_at.is_none());
    Ok(projects)
}

#[tauri::command]
pub fn list_archived_projects() -> Result<Vec<ProjectInfo>, String> {
    let dir = projects_dir()?;
    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut projects = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|e| format!("Failed to read projects dir: {}", e))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let project_json = path.join("project.json");
        if !project_json.exists() {
            continue;
        }
        let raw = fs::read_to_string(&project_json).unwrap_or_default();
        if let Ok(info) = serde_json::from_str::<StoredProjectInfo>(&raw) {
            if info.archived_at.is_some() {
                projects.push(project_info_from_stored(&path, info));
            }
        }
    }

    projects.sort_by_key(|p| p.order);
    Ok(projects)
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn create_project(
    name: String,
    description: String,
    prompt: String,
    icon: String,
    color: String,
    preferred_provider: Option<String>,
    preferred_model: Option<String>,
    working_dirs: Vec<String>,
    use_worktrees: bool,
) -> Result<ProjectInfo, String> {
    if name.trim().is_empty() {
        return Err("Project name must not be empty".to_string());
    }

    let base = projects_dir()?;
    let slug = slugify(&name);

    // Determine final directory name, avoiding collisions
    let mut dir_name = slug.clone();
    if base.join(&dir_name).exists() {
        let mut counter = 2u32;
        loop {
            dir_name = format!("{}-{}", slug, counter);
            if !base.join(&dir_name).exists() {
                break;
            }
            counter += 1;
        }
    }

    let existing_count = if base.exists() {
        fs::read_dir(&base)
            .map(|entries| entries.flatten().filter(|e| e.path().is_dir()).count())
            .unwrap_or(0)
    } else {
        0
    };

    let dir = base.join(&dir_name);
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create project directory: {}", e))?;

    let now = now_timestamp();
    let stored = StoredProjectInfo {
        id: generate_id(),
        name,
        description,
        prompt,
        icon,
        color,
        preferred_provider,
        preferred_model,
        working_dirs,
        use_worktrees,
        order: existing_count as i32,
        archived_at: None,
        created_at: now.clone(),
        updated_at: now,
    };

    let project_path = dir.join("project.json");
    let json = serde_json::to_string_pretty(&stored)
        .map_err(|e| format!("Failed to serialize project: {}", e))?;
    fs::write(&project_path, json).map_err(|e| format!("Failed to write project.json: {}", e))?;

    Ok(project_info_from_stored(&dir, stored))
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn update_project(
    id: String,
    name: String,
    description: String,
    prompt: String,
    icon: String,
    color: String,
    preferred_provider: Option<String>,
    preferred_model: Option<String>,
    working_dirs: Vec<String>,
    use_worktrees: bool,
) -> Result<ProjectInfo, String> {
    if name.trim().is_empty() {
        return Err("Project name must not be empty".to_string());
    }

    let (dir, existing) = find_project_by_id(&id)?;

    let stored = StoredProjectInfo {
        id: existing.id,
        name,
        description,
        prompt,
        icon,
        color,
        preferred_provider,
        preferred_model,
        working_dirs,
        use_worktrees,
        order: existing.order,
        archived_at: existing.archived_at,
        created_at: existing.created_at,
        updated_at: now_timestamp(),
    };

    let project_path = dir.join("project.json");
    let json = serde_json::to_string_pretty(&stored)
        .map_err(|e| format!("Failed to serialize project: {}", e))?;
    fs::write(&project_path, json).map_err(|e| format!("Failed to write project.json: {}", e))?;

    Ok(project_info_from_stored(&dir, stored))
}

#[tauri::command]
pub fn delete_project(id: String) -> Result<(), String> {
    let (dir, _) = find_project_by_id(&id)?;
    fs::remove_dir_all(&dir).map_err(|e| format!("Failed to delete project: {}", e))?;
    Ok(())
}

#[tauri::command]
pub fn reorder_projects(order: Vec<(String, i32)>) -> Result<(), String> {
    for (id, new_order) in order {
        let (dir, mut stored) = find_project_by_id(&id)?;
        stored.order = new_order;
        let project_path = dir.join("project.json");
        let json = serde_json::to_string_pretty(&stored)
            .map_err(|e| format!("Failed to serialize project: {}", e))?;
        fs::write(&project_path, json)
            .map_err(|e| format!("Failed to write project.json: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
pub fn get_project(id: String) -> Result<ProjectInfo, String> {
    let (dir, info) = find_project_by_id(&id)?;
    Ok(project_info_from_stored(&dir, info))
}

#[tauri::command]
pub fn archive_project(id: String) -> Result<(), String> {
    let base = projects_dir()?;
    if !base.exists() {
        return Err("Projects directory not found".to_string());
    }

    let entries = fs::read_dir(&base).map_err(|e| format!("Failed to read projects dir: {}", e))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let project_json = path.join("project.json");
        if !project_json.exists() {
            continue;
        }
        let raw = fs::read_to_string(&project_json).unwrap_or_default();
        if let Ok(mut info) = serde_json::from_str::<StoredProjectInfo>(&raw) {
            if info.id == id {
                info.archived_at = Some(now_timestamp());
                let json = serde_json::to_string_pretty(&info)
                    .map_err(|e| format!("Failed to serialize: {}", e))?;
                fs::write(&project_json, json).map_err(|e| format!("Failed to write: {}", e))?;
                return Ok(());
            }
        }
    }

    Err(format!("Project with id \"{}\" not found", id))
}

#[tauri::command]
pub fn restore_project(id: String) -> Result<(), String> {
    let base = projects_dir()?;
    if !base.exists() {
        return Err("Projects directory not found".to_string());
    }

    let entries = fs::read_dir(&base).map_err(|e| format!("Failed to read projects dir: {}", e))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let project_json = path.join("project.json");
        if !project_json.exists() {
            continue;
        }
        let raw = fs::read_to_string(&project_json).unwrap_or_default();
        if let Ok(mut info) = serde_json::from_str::<StoredProjectInfo>(&raw) {
            if info.id == id {
                info.archived_at = None;
                let json = serde_json::to_string_pretty(&info)
                    .map_err(|e| format!("Failed to serialize: {}", e))?;
                fs::write(&project_json, json).map_err(|e| format!("Failed to write: {}", e))?;
                return Ok(());
            }
        }
    }

    Err(format!("Project with id \"{}\" not found", id))
}

#[cfg(test)]
mod tests {
    use super::{project_artifacts_dir, StoredProjectInfo};
    use std::path::Path;

    #[test]
    fn deserializes_legacy_single_working_dir() {
        let project: StoredProjectInfo = serde_json::from_str(
            r##"{
              "id": "project-1",
              "name": "Legacy",
              "description": "",
              "prompt": "",
              "icon": "📁",
              "color": "#000000",
              "preferredProvider": null,
              "preferredModel": null,
              "workingDir": "/tmp/legacy",
              "useWorktrees": false,
              "createdAt": "now",
              "updatedAt": "now"
            }"##,
        )
        .expect("legacy project");

        assert_eq!(project.working_dirs, vec!["/tmp/legacy"]);
    }

    #[test]
    fn builds_project_artifacts_dir_inside_project_storage() {
        assert_eq!(
            project_artifacts_dir(Path::new("/Users/test/.goose/projects/sample-project")),
            "/Users/test/.goose/projects/sample-project/artifacts"
        );
    }
}
