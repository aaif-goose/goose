mod inspector;

pub use inspector::ContractInspector;

use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::Match;
use serde::Deserialize;

use crate::hints::load_hints::find_git_root;

pub const CONTRACT_FILENAMES: [&str; 2] = ["agent.yaml", "agent.yml"];

/// Repository-level contract for agents, declared in `agent.yaml` at (or above)
/// the working directory up to the git root. Unlike `.goosehints`/`AGENTS.md`,
/// its `forbidden` rules are enforced: matching tool calls are denied.
#[derive(Debug, Default, Deserialize)]
pub struct AgentContract {
    #[serde(default)]
    pub setup: Vec<String>,
    #[serde(default)]
    pub verify: Vec<String>,
    #[serde(default)]
    pub forbidden_paths: Vec<String>,
    #[serde(default)]
    pub forbidden_commands: Vec<String>,
    #[serde(default)]
    pub context: ContractContext,
    #[serde(default)]
    pub instructions: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct ContractContext {
    /// Extra ignore patterns, merged into the hint gitignore so @-imports and
    /// hint discovery skip them.
    #[serde(default)]
    pub ignore: Vec<String>,
}

pub struct LoadedContract {
    pub contract: AgentContract,
    /// Directory containing the contract file; patterns are relative to it.
    pub root: PathBuf,
    path_overrides: Gitignore,
    /// Patterns without a path separator; they also apply to paths outside
    /// the contract root (e.g. an agent writing to `~/.env`).
    basename_patterns: Vec<String>,
}

impl AgentContract {
    /// Find and load the nearest `agent.yaml`/`agent.yml` walking from `start_dir`
    /// up to the git root. Without a git root only `start_dir` is checked.
    /// A present but unparseable file disables the contract (logged).
    pub fn discover(start_dir: &Path) -> Option<LoadedContract> {
        let git_root = find_git_root(start_dir);
        let mut dir = start_dir;
        loop {
            for name in CONTRACT_FILENAMES {
                let path = dir.join(name);
                if path.is_file() {
                    return match Self::load(&path) {
                        Ok(loaded) => Some(loaded),
                        Err(e) => {
                            tracing::warn!("failed to load {}: {}", path.display(), e);
                            None
                        }
                    };
                }
            }
            if Some(dir) == git_root || git_root.is_none() {
                break;
            }
            match dir.parent() {
                Some(parent) => dir = parent,
                None => break,
            }
        }
        None
    }

    pub fn load(path: &Path) -> Result<LoadedContract> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let contract: AgentContract = serde_yaml::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        let root = path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));

        let root = root.canonicalize().unwrap_or(root);

        let mut builder = GitignoreBuilder::new(&root);
        let mut dir_prefixes = Vec::new();
        for pattern in &contract.forbidden_paths {
            builder
                .add_line(None, pattern)
                .with_context(|| format!("invalid forbidden_paths pattern {:?}", pattern))?;
            if let Some(prefix) = pattern.strip_suffix("/**") {
                dir_prefixes.push(prefix.to_string());
            }
        }
        for prefix in &dir_prefixes {
            builder
                .add_line(None, prefix)
                .with_context(|| format!("invalid forbidden_paths pattern {:?}", prefix))?;
        }
        let path_overrides = builder.build()?;

        let basename_patterns = contract
            .forbidden_paths
            .iter()
            .filter(|p| !p.contains('/') && !p.contains('\\'))
            .cloned()
            .collect();

        Ok(LoadedContract {
            contract,
            root,
            path_overrides,
            basename_patterns,
        })
    }

    /// Render the contract for the system prompt so the agent knows the rules
    /// up front instead of discovering them through denials.
    pub fn prompt_section(&self) -> Option<String> {
        if self.setup.is_empty()
            && self.verify.is_empty()
            && self.forbidden_paths.is_empty()
            && self.forbidden_commands.is_empty()
            && self.instructions.trim().is_empty()
        {
            return None;
        }
        let mut out = String::from(
            "### Repository Contract (agent.yaml)\n\
             The repository declares a machine-readable agent contract. \
             Forbidden rules are enforced: matching tool calls are rejected.\n",
        );
        let push_list = |out: &mut String, label: &str, items: &[String]| {
            if !items.is_empty() {
                out.push_str(&format!("- {}: {}\n", label, items.join(", ")));
            }
        };
        push_list(&mut out, "Setup commands", &self.setup);
        push_list(&mut out, "Verify before finishing", &self.verify);
        push_list(&mut out, "Forbidden paths", &self.forbidden_paths);
        push_list(&mut out, "Forbidden commands", &self.forbidden_commands);
        if !self.instructions.trim().is_empty() {
            out.push_str(&format!("\n{}\n", self.instructions.trim()));
        }
        Some(out)
    }
}

/// Lexically resolve `.`/`..` without touching the filesystem so candidates
/// like `../.env` cannot escape the contract root by prefix trickery.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Canonicalize the deepest existing ancestor so paths through symlinks
/// (e.g. `alias -> secrets`) resolve to their real location before matching.
fn resolve_symlinks(path: &Path) -> PathBuf {
    let mut existing = path.to_path_buf();
    let mut remainder = Vec::new();
    while !existing.exists() {
        match existing.file_name() {
            Some(name) => {
                remainder.push(name.to_os_string());
                existing.pop();
            }
            None => return path.to_path_buf(),
        }
    }
    let mut out = existing.canonicalize().unwrap_or(existing);
    for part in remainder.iter().rev() {
        out.push(part);
    }
    out
}

fn strip_file_scheme(candidate: &str) -> &str {
    let Some(rest) = candidate.strip_prefix("file://") else {
        return candidate;
    };
    if rest.len() >= 3 && rest.as_bytes()[1].is_ascii_alphabetic() && rest.as_bytes()[2] == b':' {
        rest.strip_prefix('/').unwrap_or(rest)
    } else {
        rest
    }
}

fn matched_pattern(matcher: &Gitignore, path: &Path) -> Option<String> {
    match matcher.matched_path_or_any_parents(path, path.is_dir()) {
        Match::Ignore(glob) => Some(glob.original().to_string()),
        _ => None,
    }
}

impl LoadedContract {
    /// Resolve `candidate` (relative to the session working dir) and return the
    /// forbidden pattern it matches, if any.
    pub fn forbidden_match(&self, working_dir: &Path, candidate: &str) -> Option<String> {
        let candidate = strip_file_scheme(candidate);
        let raw = Path::new(candidate);
        let resolved = resolve_symlinks(&normalize(&if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            working_dir.join(raw)
        }));

        if resolved.starts_with(&self.root) {
            return matched_pattern(&self.path_overrides, &resolved);
        }

        if self.basename_patterns.is_empty() {
            return None;
        }
        let parent = resolved.parent()?;
        let mut builder = GitignoreBuilder::new(parent);
        let mut any = false;
        for pattern in &self.basename_patterns {
            if builder.add_line(None, pattern).is_ok() {
                any = true;
            }
        }
        if !any {
            return None;
        }
        let overrides = builder.build().ok()?;
        matched_pattern(&overrides, &resolved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    const CONTRACT_YAML: &str = r#"
setup:
  - cargo build
verify:
  - cargo fmt --check
  - cargo test
forbidden_paths:
  - .env
  - "*.pem"
  - secrets/**
  - src/generated/**
forbidden_commands:
  - rm -rf
context:
  ignore:
    - target/**
instructions: Always run verify before finishing.
"#;

    fn write_contract(dir: &Path) {
        fs::write(dir.join("agent.yaml"), CONTRACT_YAML).unwrap();
    }

    #[test]
    fn test_parse_contract() {
        let dir = TempDir::new().unwrap();
        write_contract(dir.path());
        let loaded = AgentContract::discover(dir.path()).unwrap();
        assert_eq!(loaded.contract.setup, vec!["cargo build"]);
        assert_eq!(
            loaded.contract.verify,
            vec!["cargo fmt --check", "cargo test"]
        );
        assert_eq!(loaded.contract.forbidden_commands, vec!["rm -rf"]);
        assert_eq!(loaded.contract.context.ignore, vec!["target/**"]);
    }

    #[test]
    fn test_discover_walks_to_git_root() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        write_contract(dir.path());
        let subdir = dir.path().join("a/b");
        fs::create_dir_all(&subdir).unwrap();

        let loaded = AgentContract::discover(&subdir).unwrap();
        assert_eq!(loaded.root, dir.path().canonicalize().unwrap());
    }

    #[test]
    fn test_discover_stops_at_git_root() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path().join("repo");
        fs::create_dir_all(repo.join(".git")).unwrap();
        let subdir = repo.join("sub");
        fs::create_dir(&subdir).unwrap();
        write_contract(dir.path()); // above the git root — must not be picked up

        assert!(AgentContract::discover(&subdir).is_none());
    }

    #[test]
    fn test_discover_missing_and_broken() {
        let dir = TempDir::new().unwrap();
        assert!(AgentContract::discover(dir.path()).is_none());

        fs::write(dir.path().join("agent.yaml"), "forbidden_paths: [").unwrap();
        assert!(AgentContract::discover(dir.path()).is_none());
    }

    #[test]
    fn test_forbidden_match() {
        let dir = TempDir::new().unwrap();
        write_contract(dir.path());
        let loaded = AgentContract::discover(dir.path()).unwrap();
        let wd = dir.path();

        assert_eq!(loaded.forbidden_match(wd, ".env"), Some(".env".to_string()));
        assert_eq!(
            loaded.forbidden_match(wd, "config/.env"),
            Some(".env".to_string())
        );
        assert_eq!(
            loaded.forbidden_match(wd, "secrets/api.key"),
            Some("secrets/**".to_string())
        );
        assert_eq!(
            loaded.forbidden_match(wd, "src/generated/client.rs"),
            Some("src/generated/**".to_string())
        );
        assert!(loaded.forbidden_match(wd, "src/main.rs").is_none());
    }

    #[test]
    fn test_discover_without_git_root_checks_start_dir_only() {
        let dir = TempDir::new().unwrap();
        write_contract(dir.path());
        let subdir = dir.path().join("sub");
        fs::create_dir(&subdir).unwrap();

        assert!(AgentContract::discover(dir.path()).is_some());
        assert!(AgentContract::discover(&subdir).is_none());
    }

    #[test]
    fn test_forbidden_match_dir_prefix_and_cd() {
        let dir = TempDir::new().unwrap();
        write_contract(dir.path());
        let loaded = AgentContract::discover(dir.path()).unwrap();
        let wd = dir.path();

        // `x/**` also forbids `x` itself, so `cd secrets` is denied
        assert_eq!(
            loaded.forbidden_match(wd, "secrets"),
            Some("secrets".to_string())
        );
        assert_eq!(
            loaded.forbidden_match(wd, "src/generated"),
            Some("src/generated".to_string())
        );
    }

    #[test]
    fn test_forbidden_match_file_url() {
        let dir = TempDir::new().unwrap();
        write_contract(dir.path());
        let loaded = AgentContract::discover(dir.path()).unwrap();
        let wd = dir.path();

        let file_url = format!("file://{}", wd.join("secrets/photo.png").display());
        assert!(loaded.forbidden_match(wd, &file_url).is_some());
    }

    #[test]
    fn test_forbidden_match_symlink() {
        let dir = TempDir::new().unwrap();
        write_contract(dir.path());
        let secrets = dir.path().join("secrets");
        fs::create_dir(&secrets).unwrap();
        let alias = dir.path().join("alias");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&secrets, &alias).unwrap();
        #[cfg(windows)]
        if std::os::windows::fs::symlink_dir(&secrets, &alias).is_err() {
            return; // no symlink privilege on this machine
        }

        let loaded = AgentContract::discover(dir.path()).unwrap();
        assert!(loaded.forbidden_match(dir.path(), "alias/token").is_some());
    }

    #[test]
    fn test_forbidden_match_escapes_and_outside_root() {
        let dir = TempDir::new().unwrap();
        write_contract(dir.path());
        let loaded = AgentContract::discover(dir.path()).unwrap();
        let wd = dir.path();

        // ../ escape still resolves to a .env basename outside the root
        assert_eq!(
            loaded.forbidden_match(wd, "../.env"),
            Some(".env".to_string())
        );

        // absolute path outside the root matching a basename pattern
        let outside = TempDir::new().unwrap();
        let pem = outside.path().join("server.pem");
        assert_eq!(
            loaded.forbidden_match(wd, pem.to_str().unwrap()),
            Some("*.pem".to_string())
        );

        // non-basename patterns do not apply outside the root
        let secrets = outside.path().join("secrets/x.txt");
        assert!(loaded
            .forbidden_match(wd, secrets.to_str().unwrap())
            .is_none());
    }

    #[test]
    fn test_prompt_section() {
        let dir = TempDir::new().unwrap();
        write_contract(dir.path());
        let loaded = AgentContract::discover(dir.path()).unwrap();
        let section = loaded.contract.prompt_section().unwrap();
        assert!(section.contains("Repository Contract"));
        assert!(section.contains("cargo test"));
        assert!(section.contains(".env"));
        assert!(section.contains("Always run verify before finishing."));
    }

    #[test]
    fn test_prompt_section_empty() {
        let contract = AgentContract::default();
        assert!(contract.prompt_section().is_none());
    }
}
