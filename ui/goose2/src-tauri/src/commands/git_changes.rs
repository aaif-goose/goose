use serde::Serialize;
use std::path::Path;

use super::git::{is_git_repo, resolve_repo_path, run_git_success};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile {
    pub path: String,
    pub status: String,
    pub additions: u32,
    pub deletions: u32,
}

#[tauri::command]
pub fn get_changed_files(path: String) -> Result<Vec<ChangedFile>, String> {
    let repo_path = resolve_repo_path(&path)?;

    if !is_git_repo(&repo_path)? {
        return Ok(Vec::new());
    }

    let status_output = run_git_success(
        &repo_path,
        &["status", "--porcelain", "--untracked-files=all"],
    )?;
    if status_output.trim().is_empty() {
        return Ok(Vec::new());
    }

    let head_numstat =
        run_git_success(&repo_path, &["diff", "HEAD", "--numstat"]).unwrap_or_default();
    let head_stats = parse_numstat(&head_numstat);

    let mut files: Vec<ChangedFile> = Vec::new();

    for line in status_output.lines() {
        if line.len() < 4 {
            continue;
        }

        let index_status = line.as_bytes()[0];
        let worktree_status = line.as_bytes()[1];
        let file_path = unquote_porcelain(line[3..].trim());
        let file_path = if file_path.contains(" -> ") {
            file_path
                .split(" -> ")
                .last()
                .unwrap_or(&file_path)
                .to_string()
        } else {
            file_path
        };

        let status = parse_status_codes(index_status, worktree_status);

        let (additions, deletions) = head_stats
            .get(&file_path)
            .copied()
            .unwrap_or_else(|| count_file_lines(&repo_path, &file_path));

        files.push(ChangedFile {
            path: file_path,
            status,
            additions,
            deletions,
        });
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

fn parse_status_codes(index: u8, worktree: u8) -> String {
    if index == b'?' && worktree == b'?' {
        return "untracked".to_string();
    }
    if index == b'A' || (index == b'?' && worktree != b'?') {
        return "added".to_string();
    }
    if index == b'D' || worktree == b'D' {
        return "deleted".to_string();
    }
    if index == b'R' {
        return "renamed".to_string();
    }
    if index == b'C' {
        return "copied".to_string();
    }
    "modified".to_string()
}

fn parse_numstat(output: &str) -> std::collections::HashMap<String, (u32, u32)> {
    let mut map = std::collections::HashMap::new();
    for line in output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 {
            let additions = parts[0].parse::<u32>().unwrap_or(0);
            let deletions = parts[1].parse::<u32>().unwrap_or(0);
            let path = parts[2..].join("\t");
            let path = expand_rename_path(&path);
            map.insert(path, (additions, deletions));
        }
    }
    map
}

fn unquote_porcelain(s: &str) -> String {
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

fn expand_rename_path(path: &str) -> String {
    if let Some(brace_start) = path.find('{') {
        if let Some(brace_end) = path.find('}') {
            let prefix = &path[..brace_start];
            let inner = &path[brace_start + 1..brace_end];
            let suffix = &path[brace_end + 1..];
            let new_name = inner.split(" => ").last().unwrap_or(inner);
            return format!("{}{}{}", prefix, new_name, suffix);
        }
    }
    if path.contains(" => ") {
        path.split(" => ").last().unwrap_or(path).to_string()
    } else {
        path.to_string()
    }
}

const MAX_LINE_COUNT_SIZE: u64 = 1024 * 1024;

fn count_file_lines(repo_path: &Path, file_path: &str) -> (u32, u32) {
    let full = repo_path.join(file_path);
    let meta = match std::fs::metadata(&full) {
        Ok(m) => m,
        Err(_) => return (0, 0),
    };
    if meta.len() > MAX_LINE_COUNT_SIZE {
        return (0, 0);
    }
    match std::fs::read_to_string(&full) {
        Ok(contents) => {
            let count = contents.lines().count() as u32;
            (count, 0)
        }
        Err(_) => (0, 0),
    }
}
