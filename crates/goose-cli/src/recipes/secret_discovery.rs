use crate::recipes::search_recipe::load_recipe_file;
use goose::agents::extension::ExtensionConfig;
use goose::recipe::Recipe;
use regex::{NoExpand, Regex};
use std::collections::HashSet;

/// Represents a secret requirement discovered from a recipe extension
#[derive(Debug, Clone, PartialEq)]
pub struct SecretRequirement {
    /// The environment variable name (e.g., "GITHUB_TOKEN")
    pub key: String,
    /// The name of the extension that requires this secret
    pub extension_name: String,
}

impl SecretRequirement {
    pub fn new(extension_name: String, key: String) -> Self {
        Self {
            key,
            extension_name,
        }
    }

    /// Returns a human-readable description of what this secret is for
    pub fn description(&self) -> String {
        format!("Required by {} extension", self.extension_name)
    }
}

/// Discovers all secrets required by MCP extensions in a recipe and its sub-recipes
///
/// This function recursively scans the recipe and all its sub-recipes for extensions
/// and collects their declared env_keys, creating SecretRequirement structs for each
/// unique environment variable.
///
/// # Arguments
/// * `recipe` - The recipe to analyze for secret requirements
///
/// # Returns
/// A vector of SecretRequirement objects, deduplicated by key name
pub fn discover_recipe_secrets(recipe: &Recipe) -> Vec<SecretRequirement> {
    let mut visited_recipes = HashSet::new();
    discover_recipe_secrets_recursive(recipe, &mut visited_recipes)
}

/// Extract secrets from a list of extensions
fn extract_secrets_from_extensions(
    extensions: &[ExtensionConfig],
    seen_keys: &mut HashSet<String>,
) -> Vec<SecretRequirement> {
    let mut secrets = Vec::new();

    for ext in extensions {
        let (extension_name, env_keys) = match ext {
            ExtensionConfig::Stdio { name, env_keys, .. } => (name, env_keys),
            ExtensionConfig::StreamableHttp { name, env_keys, .. } => (name, env_keys),
            ExtensionConfig::Builtin { name, .. } => (name, &Vec::new()),
            ExtensionConfig::Platform { name, .. } => (name, &Vec::new()),
            ExtensionConfig::Frontend { name, .. } => (name, &Vec::new()),
            ExtensionConfig::InlinePython { name, .. } => (name, &Vec::new()),
            // SSE is unsupported - skip
            ExtensionConfig::Sse { name, .. } => {
                tracing::warn!(name = %name, "SSE is unsupported, skipping");
                continue;
            }
        };

        for key in env_keys {
            if seen_keys.insert(key.clone()) {
                let secret_req = SecretRequirement::new(extension_name.clone(), key.clone());
                secrets.push(secret_req);
            }
        }
    }

    secrets
}

/// Internal recursive function (depth-first search) to discover secrets nested in sub-recipes
/// This is future-proofing for a time when we have more than one-level of sub-recipe nesting
fn discover_recipe_secrets_recursive(
    recipe: &Recipe,
    visited_recipes: &mut HashSet<String>,
) -> Vec<SecretRequirement> {
    let mut secrets: Vec<SecretRequirement> = Vec::new();
    let mut seen_keys = HashSet::new();

    if let Some(extensions) = &recipe.extensions {
        secrets.extend(extract_secrets_from_extensions(extensions, &mut seen_keys));
    }

    if let Some(sub_recipes) = &recipe.sub_recipes {
        for sub_recipe in sub_recipes {
            if visited_recipes.contains(&sub_recipe.path) {
                continue;
            }
            visited_recipes.insert(sub_recipe.path.clone());

            match load_sub_recipe(&sub_recipe.path) {
                Ok((loaded_recipe, parent_dir)) => {
                    let sub_secrets =
                        discover_sub_recipe_secrets(&loaded_recipe, &parent_dir, visited_recipes);
                    for sub_secret in sub_secrets {
                        if seen_keys.insert(sub_secret.key.clone()) {
                            secrets.push(sub_secret);
                        }
                    }
                }
                Err(_) => {
                    continue;
                }
            }
        }
    }

    secrets
}

/// Discovers secrets from a loaded sub-recipe, resolving `{{ recipe_dir }}` in nested
/// sub-recipe paths so they can be loaded without triggering confusing lookup failures.
fn discover_sub_recipe_secrets(
    recipe: &Recipe,
    parent_dir: &str,
    visited_recipes: &mut HashSet<String>,
) -> Vec<SecretRequirement> {
    let re = Regex::new(r"\{\{\s*recipe_dir\s*\}\}").expect("valid regex");
    let mut resolved = recipe.clone();
    if let Some(ref mut sub_recipes) = resolved.sub_recipes {
        for sr in sub_recipes.iter_mut() {
            sr.path = re.replace_all(&sr.path, NoExpand(parent_dir)).into_owned();
        }
    }
    discover_recipe_secrets_recursive(&resolved, visited_recipes)
}

/// Loads a recipe from a file path for sub-recipe secret discovery.
///
/// Returns the parsed recipe along with its parent directory path, which is
/// needed to resolve `{{ recipe_dir }}` in any nested sub-recipe paths.
fn load_sub_recipe(recipe_path: &str) -> Result<(Recipe, String), Box<dyn std::error::Error>> {
    let recipe_file = load_recipe_file(recipe_path)?;
    let recipe: Recipe = serde_yaml::from_str(&recipe_file.content)?;
    let parent_dir = recipe_file.parent_dir.display().to_string();
    Ok((recipe, parent_dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use goose::agents::extension::{Envs, ExtensionConfig};
    use goose::recipe::Recipe;
    use std::collections::HashMap;

    fn create_test_recipe_with_extensions() -> Recipe {
        Recipe {
            version: "1.0.0".to_string(),
            title: "Test Recipe".to_string(),
            description: "A test recipe with MCP extensions".to_string(),
            instructions: Some("Test instructions".to_string()),
            prompt: None,
            extensions: Some(vec![
                ExtensionConfig::StreamableHttp {
                    name: "github-mcp".to_string(),
                    uri: "http://localhost:8080/mcp".to_string(),
                    envs: Envs::new(HashMap::new()),
                    env_keys: vec!["GITHUB_TOKEN".to_string(), "GITHUB_API_URL".to_string()],
                    description: "github-mcp".to_string(),
                    timeout: None,
                    socket: None,
                    bundled: None,
                    available_tools: Vec::new(),
                    headers: HashMap::new(),
                },
                ExtensionConfig::Stdio {
                    name: "slack-mcp".to_string(),
                    cmd: "slack-mcp".to_string(),
                    args: vec![],
                    envs: Envs::new(HashMap::new()),
                    env_keys: vec!["SLACK_TOKEN".to_string()],
                    timeout: None,
                    description: "slack-mcp".to_string(),
                    bundled: None,
                    available_tools: Vec::new(),
                },
                ExtensionConfig::Builtin {
                    name: "builtin-ext".to_string(),
                    display_name: None,
                    description: "builtin-ext".to_string(),
                    timeout: None,
                    bundled: None,
                    available_tools: Vec::new(),
                },
            ]),
            settings: None,
            activities: None,
            author: None,
            parameters: None,
            response: None,
            sub_recipes: None,
            retry: None,
        }
    }

    #[test]
    fn test_discover_recipe_secrets() {
        let recipe = create_test_recipe_with_extensions();
        let secrets = discover_recipe_secrets(&recipe);

        assert_eq!(secrets.len(), 3);

        let github_token = secrets.iter().find(|s| s.key == "GITHUB_TOKEN").unwrap();
        assert_eq!(github_token.key, "GITHUB_TOKEN");
        assert_eq!(github_token.extension_name, "github-mcp");
        assert_eq!(
            github_token.description(),
            "Required by github-mcp extension"
        );

        let github_api = secrets.iter().find(|s| s.key == "GITHUB_API_URL").unwrap();
        assert_eq!(github_api.key, "GITHUB_API_URL");
        assert_eq!(github_api.extension_name, "github-mcp");

        let slack_token = secrets.iter().find(|s| s.key == "SLACK_TOKEN").unwrap();
        assert_eq!(slack_token.key, "SLACK_TOKEN");
        assert_eq!(slack_token.extension_name, "slack-mcp");
    }

    #[test]
    fn test_discover_recipe_secrets_empty_recipe() {
        let recipe = Recipe {
            version: "1.0.0".to_string(),
            title: "Empty Recipe".to_string(),
            description: "A recipe with no extensions".to_string(),
            instructions: Some("Test instructions".to_string()),
            prompt: None,
            extensions: None,
            settings: None,
            activities: None,
            author: None,
            parameters: None,
            response: None,
            sub_recipes: None,
            retry: None,
        };

        let secrets = discover_recipe_secrets(&recipe);
        assert_eq!(secrets.len(), 0);
    }

    #[test]
    fn test_discover_recipe_secrets_deduplication() {
        let recipe = Recipe {
            version: "1.0.0".to_string(),
            title: "Test Recipe".to_string(),
            description: "A test recipe with duplicate secrets".to_string(),
            instructions: Some("Test instructions".to_string()),
            prompt: None,
            extensions: Some(vec![
                ExtensionConfig::StreamableHttp {
                    name: "service-a".to_string(),
                    uri: "http://localhost:8080/mcp".to_string(),
                    envs: Envs::new(HashMap::new()),
                    env_keys: vec!["API_KEY".to_string()],
                    description: "service-a".to_string(),
                    timeout: None,
                    socket: None,
                    bundled: None,
                    available_tools: Vec::new(),
                    headers: HashMap::new(),
                },
                ExtensionConfig::Stdio {
                    name: "service-b".to_string(),
                    cmd: "service-b".to_string(),
                    args: vec![],
                    envs: Envs::new(HashMap::new()),
                    env_keys: vec!["API_KEY".to_string()], // Same original key, different extension
                    timeout: None,
                    description: "service-b".to_string(),
                    bundled: None,
                    available_tools: Vec::new(),
                },
            ]),
            settings: None,
            activities: None,
            author: None,
            parameters: None,
            response: None,
            sub_recipes: None,
            retry: None,
        };

        let secrets = discover_recipe_secrets(&recipe);
        assert_eq!(secrets.len(), 1);

        let api_key = secrets.iter().find(|s| s.key == "API_KEY").unwrap();
        assert_eq!(api_key.key, "API_KEY");
        assert!(api_key.extension_name == "service-a" || api_key.extension_name == "service-b");
    }

    #[test]
    fn test_secret_requirement_creation() {
        let req = SecretRequirement::new("test-ext".to_string(), "API_TOKEN".to_string());

        assert_eq!(req.key, "API_TOKEN");
        assert_eq!(req.extension_name, "test-ext");
        assert_eq!(req.description(), "Required by test-ext extension");
    }

    #[test]
    fn test_discover_recipe_secrets_with_sub_recipes() {
        use goose::recipe::SubRecipe;

        let recipe = Recipe {
            version: "1.0.0".to_string(),
            title: "Parent Recipe".to_string(),
            description: "A recipe with sub-recipes".to_string(),
            instructions: Some("Test instructions".to_string()),
            prompt: None,
            extensions: Some(vec![ExtensionConfig::StreamableHttp {
                name: "parent-ext".to_string(),
                uri: "http://localhost:8080/mcp".to_string(),
                envs: Envs::new(HashMap::new()),
                env_keys: vec!["PARENT_TOKEN".to_string()],
                description: "parent-ext".to_string(),
                timeout: None,
                socket: None,
                bundled: None,
                available_tools: Vec::new(),
                headers: HashMap::new(),
            }]),
            sub_recipes: Some(vec![SubRecipe {
                name: "child-recipe".to_string(),
                path: "path/to/child.yaml".to_string(),
                values: None,
                sequential_when_repeated: false,
                description: None,
            }]),
            settings: None,
            activities: None,
            author: None,
            parameters: None,
            response: None,
            retry: None,
        };

        let secrets = discover_recipe_secrets(&recipe);

        assert_eq!(secrets.len(), 1);

        let parent_secret = secrets.iter().find(|s| s.key == "PARENT_TOKEN").unwrap();
        assert_eq!(parent_secret.extension_name, "parent-ext");
    }
}
