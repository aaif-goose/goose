use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::check::Check;

/// Discovered set of checks for a review request.
///
/// `checks` contains both author-defined checks (from `.agents/checks/*.md`)
/// and virtual checks auto-derived from `**/.agents/REVIEW.md` files so their
/// findings can be attributed back to that file.
#[derive(Debug, Default, Clone)]
pub struct DiscoveredReview {
    /// All applicable checks, sorted by name. Closer scopes shadow same-named
    /// checks from broader scopes (project root, then home/global).
    pub checks: Vec<Check>,
}

/// Virtual check name prefix for `REVIEW.md`-derived checks. Findings emitted
/// by these checks should be attributed to the originating `REVIEW.md`.
pub const REVIEW_MD_CHECK_PREFIX: &str = "repo-rules";

fn review_md_virtual_check_name(scope_dir: &str) -> String {
    if scope_dir.is_empty() {
        REVIEW_MD_CHECK_PREFIX.to_string()
    } else {
        format!("{REVIEW_MD_CHECK_PREFIX}:{scope_dir}")
    }
}

fn synthesize_review_md_check(scope_dir: &str, path: &Path, body: &str) -> Check {
    let scope_label = if scope_dir.is_empty() {
        "the entire repository".to_string()
    } else {
        format!("files under `{scope_dir}/`")
    };
    let intro = format!(
        "You are enforcing the project's `REVIEW.md` rules for {scope_label}.\n\n\
         These rules were authored in `{}`. Apply them strictly to the diff.\n\n\
         ---\n\n",
        path.display(),
    );
    Check {
        name: review_md_virtual_check_name(scope_dir),
        description: Some(format!("Auto-derived from {}", path.display())),
        model: None,
        turn_limit: None,
        tools: None,
        severity_default: None,
        path: path.to_path_buf(),
        scope_dir: scope_dir.to_string(),
        body: format!("{intro}{}", body.trim()),
    }
}

/// Locations searched for global checks, in priority order.
///
/// The first existing directory wins for a given check name; closer scopes
/// (repo root, then sub-trees) shadow these globals when names collide.
pub fn global_checks_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = dirs_home() {
        dirs.push(home.join(".config").join("goose").join("checks"));
        dirs.push(home.join(".config").join("agents").join("checks"));
    }
    dirs
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

/// Discover all checks and REVIEW.md files relevant to `touched_files`.
///
/// `repo_root` is the absolute path to the repository root; `touched_files` is
/// the list of repo-relative file paths changed in the diff. When `touched_files`
/// is empty, only repo-root and global locations are considered.
pub fn discover(repo_root: &Path, touched_files: &[String]) -> Result<DiscoveredReview> {
    discover_with_globals(repo_root, touched_files, &global_checks_dirs())
}

fn discover_with_globals(
    repo_root: &Path,
    touched_files: &[String],
    global_dirs: &[PathBuf],
) -> Result<DiscoveredReview> {
    let scope_dirs = candidate_scope_dirs(touched_files);

    // Collect raw checks keyed by name with explicit per-source priority so
    // closer/more-specific sources shadow broader ones. Globals get priority
    // 0 so a repo-root check (priority 1) of the same name always wins.
    let mut by_name: BTreeMap<String, (usize, Check)> = BTreeMap::new();
    let mut record = |check: Check, priority: usize| {
        by_name
            .entry(check.name.clone())
            .and_modify(|(existing_priority, existing)| {
                if priority > *existing_priority {
                    *existing = check.clone();
                    *existing_priority = priority;
                }
            })
            .or_insert((priority, check));
    };

    for dir in global_dirs {
        for check in read_checks_dir(dir, "", LoadMode::Lenient)? {
            record(check, 0);
        }
    }

    let root_dir = repo_root.join(".agents").join("checks");
    for check in read_checks_dir(&root_dir, "", LoadMode::Strict)? {
        record(check, scope_priority(""));
    }

    for scope in &scope_dirs {
        let dir = repo_root.join(scope).join(".agents").join("checks");
        for check in read_checks_dir(&dir, scope, LoadMode::Strict)? {
            let p = scope_priority(scope);
            record(check, p);
        }
    }

    let root_review = repo_root.join(".agents").join("REVIEW.md");
    if root_review.is_file() {
        let body = fs::read_to_string(&root_review)
            .with_context(|| format!("read REVIEW.md {}", root_review.display()))?;
        let check = synthesize_review_md_check("", &root_review, &body);
        record(check, scope_priority(""));
    }
    for scope in &scope_dirs {
        let path = repo_root.join(scope).join(".agents").join("REVIEW.md");
        if path.is_file() {
            let body = fs::read_to_string(&path)
                .with_context(|| format!("read REVIEW.md {}", path.display()))?;
            let check = synthesize_review_md_check(scope, &path, &body);
            let p = scope_priority(scope);
            record(check, p);
        }
    }

    let mut checks: Vec<Check> = by_name.into_values().map(|(_, c)| c).collect();
    checks.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(DiscoveredReview { checks })
}

/// Whether parse errors in a check directory should fail the run or be skipped
/// with a warning. Repo-local checks are loaded `Strict` because authors should
/// see their own broken frontmatter; global directories are loaded `Lenient`
/// because they often contain `README.md` and similar non-check Markdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoadMode {
    Strict,
    Lenient,
}

fn read_checks_dir(dir: &Path, scope_dir: &str, mode: LoadMode) -> Result<Vec<Check>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let entries =
        fs::read_dir(dir).with_context(|| format!("read checks dir {}", dir.display()))?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        // README.md is conventionally the directory's docs, not a check.
        if path.file_name().and_then(|s| s.to_str()) == Some("README.md") {
            continue;
        }
        let parsed = Check::from_path(&path).and_then(|mut check| {
            if mode == LoadMode::Strict {
                check.validate_name_matches_filename()?;
            }
            check.scope_dir = scope_dir.to_string();
            Ok(check)
        });
        match parsed {
            Ok(check) => out.push(check),
            Err(e) => match mode {
                LoadMode::Strict => return Err(e),
                LoadMode::Lenient => {
                    eprintln!("goose review: skipping {}: {e}", path.display());
                }
            },
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Priority of a scope for shadowing checks/overrides. Higher = closer.
fn scope_priority(scope_dir: &str) -> usize {
    if scope_dir.is_empty() {
        1
    } else {
        2 + scope_dir.split('/').count()
    }
}

/// Compute the set of in-repo scope directories whose `.agents/` may contain
/// checks or REVIEW.md applicable to the touched files.
///
/// For `api/v2/foo.rs` this yields `["api", "api/v2"]`.
pub fn candidate_scope_dirs(touched_files: &[String]) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    for file in touched_files {
        let normalized = file.replace('\\', "/");
        let dir = Path::new(&normalized).parent();
        let Some(dir) = dir else { continue };
        let parts: Vec<&str> = dir
            .to_str()
            .unwrap_or_default()
            .split('/')
            .filter(|p| !p.is_empty() && *p != ".")
            .collect();
        for i in 1..=parts.len() {
            seen.insert(parts[..i].join("/"));
        }
    }
    seen.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn candidate_scopes_walks_parents() {
        let scopes = candidate_scope_dirs(&[
            "api/v2/foo.rs".into(),
            "api/v2/bar.rs".into(),
            "README.md".into(),
        ]);
        assert_eq!(scopes, vec!["api".to_string(), "api/v2".to_string()]);
    }

    #[test]
    fn discovers_root_and_scoped_checks() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        write(
            &root.join(".agents/checks/sql.md"),
            "---\nname: sql\ndescription: root sql\n---\nbody",
        );
        write(
            &root.join("api/.agents/checks/auth.md"),
            "---\nname: auth\n---\nauth body",
        );

        let result = discover_with_globals(root, &["api/users.rs".to_string()], &[]).unwrap();
        let names: Vec<_> = result.checks.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["auth", "sql"]);
    }

    #[test]
    fn closer_scope_overrides_same_named_check() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        write(
            &root.join(".agents/checks/perf.md"),
            "---\nname: perf\ndescription: root\n---\nroot body",
        );
        write(
            &root.join("api/.agents/checks/perf.md"),
            "---\nname: perf\ndescription: scoped\n---\nscoped body",
        );

        let result = discover_with_globals(root, &["api/users.rs".to_string()], &[]).unwrap();
        assert_eq!(result.checks.len(), 1);
        let perf = &result.checks[0];
        assert_eq!(perf.scope_dir, "api");
        assert_eq!(perf.body, "scoped body");
    }

    #[test]
    fn synthesizes_virtual_checks_for_review_md_at_each_scope() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        write(&root.join(".agents/REVIEW.md"), "root rules");
        write(&root.join("api/.agents/REVIEW.md"), "api rules");
        write(&root.join("api/v2/.agents/REVIEW.md"), "v2 rules");

        let result = discover_with_globals(root, &["api/v2/x.rs".to_string()], &[]).unwrap();
        let names: Vec<_> = result.checks.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["repo-rules", "repo-rules:api", "repo-rules:api/v2"]
        );

        let root_check = result
            .checks
            .iter()
            .find(|c| c.name == "repo-rules")
            .unwrap();
        assert!(root_check.body.contains("the entire repository"));
        assert!(root_check.body.contains("root rules"));

        let scoped = result
            .checks
            .iter()
            .find(|c| c.name == "repo-rules:api/v2")
            .unwrap();
        assert!(scoped.body.contains("files under `api/v2/`"));
        assert!(scoped.body.contains("v2 rules"));
    }

    #[test]
    fn repo_root_check_overrides_same_named_global_check() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let global = tempdir().unwrap();
        write(
            &global.path().join("perf.md"),
            "---\nname: perf\ndescription: global\n---\nglobal body",
        );
        write(
            &root.join(".agents/checks/perf.md"),
            "---\nname: perf\ndescription: repo\n---\nrepo body",
        );

        let result = discover_with_globals(root, &[], &[global.path().to_path_buf()]).unwrap();
        assert_eq!(result.checks.len(), 1);
        assert_eq!(result.checks[0].body, "repo body");
        assert_eq!(result.checks[0].description.as_deref(), Some("repo"));
    }

    #[test]
    fn user_check_named_repo_rules_is_not_overwritten_by_root_review_md() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        write(
            &root.join(".agents/checks/repo-rules.md"),
            "---\nname: repo-rules\ndescription: user\n---\nuser body",
        );
        write(&root.join(".agents/REVIEW.md"), "review rules");

        let result = discover_with_globals(root, &[], &[]).unwrap();
        let repo_rules = result
            .checks
            .iter()
            .find(|c| c.name == "repo-rules")
            .expect("repo-rules check should exist");
        assert_eq!(repo_rules.body, "user body");
    }

    #[test]
    fn skips_non_markdown_in_checks_dir() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        write(&root.join(".agents/checks/README.txt"), "ignored");
        write(
            &root.join(".agents/checks/perf.md"),
            "---\nname: perf\n---\nbody",
        );
        let result = discover_with_globals(root, &[], &[]).unwrap();
        assert_eq!(result.checks.len(), 1);
    }
}
