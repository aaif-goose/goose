use std::path::PathBuf;

use anyhow::{anyhow, Result};
use goose::recipe::{Recipe, SubRecipe};

use crate::cli::InputConfig;
use crate::recipes::print_recipe::print_recipe_info;
use crate::recipes::recipe::load_recipe;
use crate::recipes::search_recipe::load_recipe_file;

pub fn extract_recipe_info_from_cli(
    recipe_name: String,
    params: Vec<(String, String)>,
    additional_sub_recipes: Vec<String>,
    quiet: bool,
) -> Result<(InputConfig, Recipe)> {
    let mut recipe = load_recipe(&recipe_name, params.clone()).unwrap_or_else(|err| {
        eprintln!("{}: {}", console::style("Error").red().bold(), err);
        std::process::exit(1);
    });
    if !quiet {
        print_recipe_info(&recipe, params);
    }

    if !additional_sub_recipes.is_empty() {
        let mut all_sub_recipes = recipe.sub_recipes.clone().unwrap_or_default();
        for sub_recipe_name in additional_sub_recipes {
            match load_recipe_file(&sub_recipe_name) {
                Ok(recipe_file) => {
                    let name = extract_recipe_name(&sub_recipe_name);
                    let recipe_file_path = recipe_file.file_path;
                    let additional_sub_recipe = SubRecipe {
                        path: recipe_file_path.to_string_lossy().to_string(),
                        name,
                        values: None,
                        sequential_when_repeated: true,
                        description: None,
                    };
                    all_sub_recipes.push(additional_sub_recipe);
                }
                Err(e) => {
                    return Err(anyhow!(
                        "Could not retrieve sub-recipe '{}': {}",
                        sub_recipe_name,
                        e
                    ));
                }
            }
        }
        recipe.sub_recipes = Some(all_sub_recipes);
    }

    let input_config = InputConfig {
        contents: recipe.prompt.clone().filter(|s| !s.trim().is_empty()),
        additional_system_prompt: None,
    };

    Ok((input_config, recipe))
}

fn extract_recipe_name(recipe_identifier: &str) -> String {
    // If it's a path (contains / or \), extract the file stem
    if recipe_identifier.contains('/') || recipe_identifier.contains('\\') {
        PathBuf::from(recipe_identifier)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string()
    } else {
        // If it's just a name (like "weekly-updates"), use it directly
        recipe_identifier.to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_extract_recipe_info_from_cli_basic() {
        let (_temp_dir, recipe_path) = create_recipe();
        let params = vec![("name".to_string(), "my_value".to_string())];
        let recipe_name = recipe_path.to_str().unwrap().to_string();

        let (input_config, recipe) =
            extract_recipe_info_from_cli(recipe_name, params, Vec::new(), false).unwrap();
        let settings = recipe.settings;
        let sub_recipes = recipe.sub_recipes;
        let response = recipe.response;

        assert_eq!(input_config.contents, Some("test_prompt".to_string()));
        assert_eq!(
            recipe.instructions,
            Some("test_instructions my_value".to_string())
        );
        assert!(recipe
            .extensions
            .as_ref()
            .is_none_or(|e| e.iter().all(|ext| ext.name() == "summon")));

        assert!(settings.is_some());
        let settings = settings.unwrap();
        assert_eq!(settings.goose_provider, Some("test_provider".to_string()));
        assert_eq!(settings.goose_model, Some("test_model".to_string()));
        assert_eq!(settings.temperature, Some(0.7));

        assert!(sub_recipes.is_some());
        let sub_recipes = sub_recipes.unwrap();
        assert!(sub_recipes.len() == 1);
        let full_sub_recipe_path = recipe_path
            .parent()
            .unwrap()
            .join("existing_sub_recipe.yaml")
            .to_string_lossy()
            .to_string();
        assert_eq!(sub_recipes[0].path, full_sub_recipe_path);
        assert_eq!(sub_recipes[0].name, "existing_sub_recipe".to_string());
        assert!(sub_recipes[0].values.is_none());
        assert!(response.is_some());
        let response = response.unwrap();
        assert_eq!(
            response.json_schema,
            Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "result": {"type": "string"}
                }
            }))
        );
    }

    #[test]
    fn test_extract_recipe_info_from_cli_with_additional_sub_recipes() {
        let (temp_dir, recipe_path) = create_recipe();

        std::fs::create_dir_all(temp_dir.path().join("path/to")).unwrap();
        std::fs::create_dir_all(temp_dir.path().join("another")).unwrap();

        let sub_recipe1_path = temp_dir.path().join("path/to/sub_recipe1.yaml");
        let sub_recipe2_path = temp_dir.path().join("another/sub_recipe2.yaml");

        std::fs::write(&sub_recipe1_path, "title: Sub Recipe 1").unwrap();
        std::fs::write(&sub_recipe2_path, "title: Sub Recipe 2").unwrap();

        let params = vec![("name".to_string(), "my_value".to_string())];
        let recipe_name = recipe_path.to_str().unwrap().to_string();
        let additional_sub_recipes = vec![
            sub_recipe1_path.to_string_lossy().to_string(),
            sub_recipe2_path.to_string_lossy().to_string(),
        ];

        let (input_config, recipe) =
            extract_recipe_info_from_cli(recipe_name, params, additional_sub_recipes, false)
                .unwrap();
        let settings = recipe.settings;
        let sub_recipes = recipe.sub_recipes;
        let response = recipe.response;

        assert_eq!(input_config.contents, Some("test_prompt".to_string()));
        assert_eq!(
            recipe.instructions,
            Some("test_instructions my_value".to_string())
        );
        assert!(recipe
            .extensions
            .as_ref()
            .is_none_or(|e| e.iter().all(|ext| ext.name() == "summon")));

        assert!(settings.is_some());
        let settings = settings.unwrap();
        assert_eq!(settings.goose_provider, Some("test_provider".to_string()));
        assert_eq!(settings.goose_model, Some("test_model".to_string()));
        assert_eq!(settings.temperature, Some(0.7));

        assert!(sub_recipes.is_some());
        let sub_recipes = sub_recipes.unwrap();
        assert!(sub_recipes.len() == 3);
        let full_sub_recipe_path = recipe_path
            .parent()
            .unwrap()
            .join("existing_sub_recipe.yaml")
            .to_string_lossy()
            .to_string();
        assert_eq!(sub_recipes[0].path, full_sub_recipe_path);
        assert_eq!(sub_recipes[0].name, "existing_sub_recipe".to_string());
        assert!(sub_recipes[0].values.is_none());
        assert_eq!(
            sub_recipes[1].path,
            sub_recipe1_path
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .to_string()
        );
        assert_eq!(sub_recipes[1].name, "sub_recipe1".to_string());
        assert!(sub_recipes[1].values.is_none());
        assert_eq!(
            sub_recipes[2].path,
            sub_recipe2_path
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .to_string()
        );
        assert_eq!(sub_recipes[2].name, "sub_recipe2".to_string());
        assert!(sub_recipes[2].values.is_none());
        assert!(response.is_some());
        let response = response.unwrap();
        assert_eq!(
            response.json_schema,
            Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "result": {"type": "string"}
                }
            }))
        );
    }

    fn create_recipe() -> (TempDir, PathBuf) {
        let test_recipe_content = r#"
title: test_recipe
description: A test recipe
instructions: test_instructions {{name}}
prompt: test_prompt
parameters:
- key: name
  description: name
  input_type: string
  requirement: required
settings:
  goose_provider: test_provider
  goose_model: test_model
  temperature: 0.7
sub_recipes:
- path: existing_sub_recipe.yaml
  name: existing_sub_recipe
response:
  json_schema:
    type: object
    properties:
      result:
        type: string
"#;
        let sub_recipe_content = r#"
title: existing_sub_recipe
description: An existing sub recipe
instructions: sub recipe instructions
prompt: sub recipe prompt
"#;
        let temp_dir = tempfile::tempdir().unwrap();
        let recipe_path: std::path::PathBuf = temp_dir.path().join("test_recipe.yaml");
        let sub_recipe_path: std::path::PathBuf = temp_dir.path().join("existing_sub_recipe.yaml");

        std::fs::write(&recipe_path, test_recipe_content).unwrap();
        std::fs::write(&sub_recipe_path, sub_recipe_content).unwrap();
        let canonical_recipe_path = recipe_path.canonicalize().unwrap();
        (temp_dir, canonical_recipe_path)
    }
}
