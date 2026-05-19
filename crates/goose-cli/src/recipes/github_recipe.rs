use anyhow::{anyhow, Result};
use console::style;
use goose::recipe::template_recipe::parse_recipe_content;
use goose::recipe::RECIPE_FILE_EXTENSIONS;
use serde::{Deserialize, Serialize};

use goose::recipe::read_recipe_file_content::RecipeFile;
use goose::subprocess::SubprocessExt;
use std::env;
use std::fs;

use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use tar::Archive;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeInfo {
    pub name: String,
    pub source: RecipeSource,
    pub path: String,
    pub title: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecipeSource {
    Local,
    GitHub,
}

pub const GOOSE_RECIPE_GITHUB_REPO_CONFIG_KEY: &str = "GOOSE_RECIPE_GITHUB_REPO";
pub fn retrieve_recipe_from_github(
    recipe_name: &str,
    recipe_repo_full_name: &str,
) -> Result<RecipeFile> {
    println!(
        "📦 Looking for recipe \"{}\" in github repo: {}",
        recipe_name, recipe_repo_full_name
    );
    ensure_gh_authenticated()?;
    let max_attempts = 2;
    let mut last_err = None;

    for attempt in 1..=max_attempts {
        match clone_and_download_recipe(recipe_name, recipe_repo_full_name) {
            Ok(download_dir) => match read_recipe_file(&download_dir) {
                Ok((content, recipe_file_local_path)) => {
                    return Ok(RecipeFile {
                        content,
                        parent_dir: download_dir.clone(),
                        file_path: recipe_file_local_path,
                    })
                }
                Err(err) => return Err(err),
            },
            Err(err) => {
                last_err = Some(err);
            }
        }
        if attempt < max_attempts {
            clean_cloned_dirs(recipe_repo_full_name)?;
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("Unknown error occurred")))
}

fn clean_cloned_dirs(recipe_repo_full_name: &str) -> anyhow::Result<()> {
    let local_repo_path = get_local_repo_path(&env::temp_dir(), recipe_repo_full_name)?;
    if local_repo_path.exists() {
        fs::remove_dir_all(&local_repo_path)?;
    }
    Ok(())
}
fn read_recipe_file(download_dir: &Path) -> Result<(String, PathBuf)> {
    for ext in RECIPE_FILE_EXTENSIONS {
        let candidate_file_path = download_dir.join(format!("recipe.{}", ext));
        if candidate_file_path.exists() {
            let content = fs::read_to_string(&candidate_file_path)?;
            println!(
                "⬇️  Retrieved recipe file: {}",
                candidate_file_path
                    .strip_prefix(download_dir)
                    .unwrap()
                    .display()
            );
            return Ok((content, candidate_file_path));
        }
    }

    Err(anyhow::anyhow!(
        "No recipe file found in {} (looked for extensions: {:?})",
        download_dir.display(),
        RECIPE_FILE_EXTENSIONS
    ))
}

fn clone_and_download_recipe(recipe_name: &str, recipe_repo_full_name: &str) -> Result<PathBuf> {
    let local_repo_path = ensure_repo_cloned(recipe_repo_full_name)?;
    fetch_origin(&local_repo_path)?;
    get_folder_from_github(&local_repo_path, recipe_name)
}

pub fn ensure_gh_authenticated() -> Result<()> {
    // Check authentication status
    let status = Command::new("gh")
        .args(["auth", "status"])
        .set_no_window()
        .status()
        .map_err(|_| {
            anyhow::anyhow!("Failed to run `gh auth status`. Make sure you have `gh` installed.")
        })?;

    if status.success() {
        return Ok(());
    }
    println!("GitHub CLI is not authenticated. Launching `gh auth login`...");
    // Run `gh auth login` interactively
    let login_status = Command::new("gh")
        .args(["auth", "login", "--git-protocol", "https"])
        .status()
        .map_err(|_| anyhow::anyhow!("Failed to run `gh auth login`"))?;

    if !login_status.success() {
        Err(anyhow::anyhow!("Failed to authenticate using GitHub CLI."))
    } else {
        Ok(())
    }
}

fn temp_child_name(name: &str) -> String {
    let mut child = String::with_capacity(name.len());
    for ch in name.chars() {
        match ch {
            '/' | '\\' => child.push_str("__"),
            ch if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' => {
                child.push(ch)
            }
            _ => child.push('_'),
        }
    }

    if child.is_empty() {
        "_".to_string()
    } else {
        child
    }
}

fn get_local_repo_path(
    local_repo_parent_path: &Path,
    recipe_repo_full_name: &str,
) -> Result<PathBuf> {
    let (owner, repo_name) = recipe_repo_full_name
        .split_once('/')
        .ok_or_else(|| anyhow::anyhow!("Invalid repository name format"))?;
    let local_repo_path = local_repo_parent_path
        .to_path_buf()
        .join(temp_child_name(&format!("{owner}/{repo_name}")));
    Ok(local_repo_path)
}

fn ensure_repo_cloned(recipe_repo_full_name: &str) -> Result<PathBuf> {
    let local_repo_parent_path = env::temp_dir();
    if !local_repo_parent_path.exists() {
        std::fs::create_dir_all(local_repo_parent_path.clone())?;
    }
    let local_repo_path = get_local_repo_path(&local_repo_parent_path, recipe_repo_full_name)?;

    if local_repo_path.join(".git").exists() {
        Ok(local_repo_path)
    } else {
        let error_message: String = format!("Failed to clone repo: {}", recipe_repo_full_name);
        let status = Command::new("gh")
            .args(["repo", "clone", recipe_repo_full_name])
            .arg(&local_repo_path)
            .current_dir(local_repo_parent_path.clone())
            .set_no_window()
            .status()
            .map_err(|_: std::io::Error| anyhow::anyhow!(error_message.clone()))?;

        if status.success() {
            Ok(local_repo_path)
        } else {
            Err(anyhow::anyhow!(error_message))
        }
    }
}

fn fetch_origin(local_repo_path: &Path) -> Result<()> {
    let error_message: String = format!("Failed to fetch at {}", local_repo_path.to_str().unwrap());
    let status = Command::new("git")
        .args(["fetch", "origin"])
        .current_dir(local_repo_path)
        .set_no_window()
        .status()
        .map_err(|_| anyhow::anyhow!(error_message.clone()))?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(error_message))
    }
}

fn get_folder_from_github(local_repo_path: &Path, recipe_name: &str) -> Result<PathBuf> {
    let ref_and_path = format!("origin/main:{}", recipe_name);
    let output_dir = env::temp_dir().join(temp_child_name(recipe_name));

    if output_dir.exists() {
        fs::remove_dir_all(&output_dir)?;
    }
    fs::create_dir_all(&output_dir)?;

    let archive_output = Command::new("git")
        .args(["archive", &ref_and_path])
        .current_dir(local_repo_path)
        .stdout(Stdio::piped())
        .set_no_window()
        .spawn()?;

    let stdout = archive_output
        .stdout
        .ok_or_else(|| anyhow::anyhow!("Failed to capture stdout from git archive"))?;

    let mut archive = Archive::new(stdout);
    archive.unpack(&output_dir)?;
    list_files(&output_dir)?;

    Ok(output_dir)
}

fn list_files(dir: &Path) -> Result<()> {
    println!("{}", style("Files downloaded from github:").bold());
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            println!("  - {}", path.display());
        }
    }
    Ok(())
}

/// Lists all available recipes from a GitHub repository
pub fn list_github_recipes(repo: &str) -> Result<Vec<RecipeInfo>> {
    discover_github_recipes(repo)
}

fn discover_github_recipes(repo: &str) -> Result<Vec<RecipeInfo>> {
    use serde_json::Value;
    use std::process::Command;

    // Ensure GitHub CLI is authenticated
    ensure_gh_authenticated()?;

    // Get repository contents using GitHub CLI
    let output = Command::new("gh")
        .args(["api", &format!("repos/{}/contents", repo)])
        .set_no_window()
        .output()
        .map_err(|e| anyhow!("Failed to fetch repository contents using 'gh api' command (executed when GOOSE_RECIPE_GITHUB_REPO is configured). This requires GitHub CLI (gh) to be installed and authenticated. Error: {}", e))?;

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("GitHub API request failed: {}", error_msg));
    }

    let contents: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| anyhow!("Failed to parse GitHub API response: {}", e))?;

    let mut recipes = Vec::new();

    if let Some(items) = contents.as_array() {
        for item in items {
            if let (Some(name), Some(item_type)) = (
                item.get("name").and_then(|n| n.as_str()),
                item.get("type").and_then(|t| t.as_str()),
            ) {
                if item_type == "dir" {
                    // Check if this directory contains a recipe file
                    if let Ok(recipe_info) = check_github_directory_for_recipe(repo, name) {
                        recipes.push(recipe_info);
                    }
                }
            }
        }
    }

    Ok(recipes)
}

fn check_github_directory_for_recipe(repo: &str, dir_name: &str) -> Result<RecipeInfo> {
    use serde_json::Value;
    use std::process::Command;

    // Check directory contents for recipe files
    let output = Command::new("gh")
        .args(["api", &format!("repos/{}/contents/{}", repo, dir_name)])
        .set_no_window()
        .output()
        .map_err(|e| anyhow!("Failed to check directory contents: {}", e))?;

    if !output.status.success() {
        return Err(anyhow!("Failed to access directory: {}", dir_name));
    }

    let contents: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| anyhow!("Failed to parse directory contents: {}", e))?;

    if let Some(items) = contents.as_array() {
        for item in items {
            if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                if RECIPE_FILE_EXTENSIONS
                    .iter()
                    .any(|ext| name == format!("recipe.{}", ext))
                {
                    // Found a recipe file, get its content
                    return get_github_recipe_info(repo, dir_name, name);
                }
            }
        }
    }

    Err(anyhow!("No recipe file found in directory: {}", dir_name))
}

fn get_github_recipe_info(repo: &str, dir_name: &str, recipe_filename: &str) -> Result<RecipeInfo> {
    use serde_json::Value;
    use std::process::Command;

    // Get the recipe file content
    let output = Command::new("gh")
        .args([
            "api",
            &format!("repos/{}/contents/{}/{}", repo, dir_name, recipe_filename),
        ])
        .set_no_window()
        .output()
        .map_err(|e| anyhow!("Failed to get recipe file content: {}", e))?;

    if !output.status.success() {
        return Err(anyhow!(
            "Failed to access recipe file: {}/{}",
            dir_name,
            recipe_filename
        ));
    }

    let file_info: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| anyhow!("Failed to parse file info: {}", e))?;

    if let Some(content_b64) = file_info.get("content").and_then(|c| c.as_str()) {
        // Decode base64 content
        use base64::{engine::general_purpose, Engine as _};
        let content_bytes = general_purpose::STANDARD
            .decode(content_b64.replace('\n', ""))
            .map_err(|e| anyhow!("Failed to decode base64 content: {}", e))?;

        let content = String::from_utf8(content_bytes)
            .map_err(|e| anyhow!("Failed to convert content to string: {}", e))?;

        // Parse the recipe content
        let (recipe, _) = parse_recipe_content(&content, Some(format!("{}/{}", repo, dir_name)))?;

        return Ok(RecipeInfo {
            name: dir_name.to_string(),
            source: RecipeSource::GitHub,
            path: format!("{}/{}", repo, dir_name),
            title: Some(recipe.title),
            description: Some(recipe.description),
        });
    }

    Err(anyhow!("Failed to get recipe content from GitHub"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn local_repo_path_includes_owner_to_avoid_collisions() {
        let parent = Path::new("goose-recipes");
        let first = get_local_repo_path(parent, "owner-one/shared").unwrap();
        let second = get_local_repo_path(parent, "owner-two/shared").unwrap();

        assert_ne!(first, second);
        assert_eq!(first, parent.join("owner-one__shared"));
        assert_eq!(second, parent.join("owner-two__shared"));
    }

    #[test]
    fn temp_child_name_keeps_recipe_downloads_under_temp_dir() {
        let parent = Path::new("goose-recipes");
        let child = temp_child_name("../outside");
        let output_dir = parent.join(&child);

        assert_eq!(child, "..__outside");
        assert!(output_dir.starts_with(parent));
    }
}
