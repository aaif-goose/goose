use crate::recipe::read_recipe_file_content::RecipeFile;
use crate::recipe::template_recipe::parse_recipe_content;
use crate::recipe::{
    Recipe, RecipeParameter, RecipeParameterInputType, RecipeParameterRequirement,
    BUILT_IN_RECIPE_DIR_PARAM,
};
use anyhow::Result;
use std::collections::HashSet;

pub fn parse_and_validate_parameters(
    recipe_file_content: &str,
    recipe_dir_str: Option<String>,
) -> Result<Recipe> {
    let (recipe_template, template_variables) =
        parse_recipe_content(recipe_file_content, recipe_dir_str)?;
    let recipe_parameters = &recipe_template.parameters;
    validate_optional_parameters(recipe_parameters)?;
    validate_parameters_in_template(recipe_parameters, &template_variables)?;
    Ok(recipe_template)
}

fn validate_json_schema(schema: &serde_json::Value) -> Result<()> {
    match jsonschema::validator_for(schema) {
        Ok(_) => Ok(()),
        Err(err) => Err(anyhow::anyhow!("JSON schema validation failed: {}", err)),
    }
}

pub fn validate_recipe_template_from_file(recipe_file: &RecipeFile) -> Result<Recipe> {
    let recipe_dir = recipe_file
        .parent_dir
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Error getting recipe directory"))?
        .to_string();

    validate_recipe_template_from_content(&recipe_file.content, Some(recipe_dir))
}

pub fn validate_recipe_template_from_content(
    recipe_content: &str,
    recipe_dir: Option<String>,
) -> Result<Recipe> {
    parse_and_validate_parameters(recipe_content, recipe_dir.clone())?;
    let (recipe, _) = parse_recipe_content(recipe_content, recipe_dir)?;

    validate_prompt_or_instructions(&recipe)?;
    validate_retry_config(&recipe)?;
    if let Some(response) = &recipe.response {
        if let Some(json_schema) = &response.json_schema {
            validate_json_schema(json_schema)?;
        }
    }

    Ok(recipe)
}

fn validate_retry_config(recipe: &Recipe) -> Result<()> {
    if let Some(ref retry_config) = recipe.retry {
        if let Err(validation_error) = retry_config.validate() {
            return Err(anyhow::anyhow!(
                "Invalid retry configuration: {}",
                validation_error
            ));
        }
    }
    Ok(())
}

fn validate_prompt_or_instructions(recipe: &Recipe) -> Result<()> {
    let has_instructions = recipe
        .instructions
        .as_ref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let has_prompt = recipe
        .prompt
        .as_ref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);

    if has_instructions || has_prompt {
        return Ok(());
    }

    Err(anyhow::anyhow!(
        "Recipe must specify at least one of `instructions` or `prompt`."
    ))
}

fn validate_parameters_in_template(
    recipe_parameters: &Option<Vec<RecipeParameter>>,
    template_variables: &HashSet<String>,
) -> Result<()> {
    let mut template_variables = template_variables.clone();
    template_variables.remove(BUILT_IN_RECIPE_DIR_PARAM);

    let params = recipe_parameters.as_deref().unwrap_or_default();

    let param_keys: HashSet<String> = params.iter().map(|p| p.key.clone()).collect();

    // Structured parameters (object/array) use dot-notation in templates
    // (e.g., {{ signal.name }}), which undeclared_variables reports as
    // "signal.name". Normalize these to their root key so they match the
    // parameter definition.
    let structured_keys: HashSet<&str> = params
        .iter()
        .filter(|p| {
            matches!(
                p.input_type,
                RecipeParameterInputType::Object | RecipeParameterInputType::Array
            )
        })
        .map(|p| p.key.as_str())
        .collect();

    let normalized_template_vars: HashSet<String> = template_variables
        .iter()
        .map(|var| {
            let root = var.split('.').next().unwrap_or(var);
            if structured_keys.contains(root) {
                root.to_string()
            } else {
                var.clone()
            }
        })
        .collect();

    let missing_keys = normalized_template_vars
        .difference(&param_keys)
        .collect::<Vec<_>>();

    let extra_keys = param_keys
        .difference(&normalized_template_vars)
        .collect::<Vec<_>>();

    if missing_keys.is_empty() && extra_keys.is_empty() {
        return Ok(());
    }

    let mut message = String::new();

    if !missing_keys.is_empty() {
        message.push_str(&format!(
            "Missing definitions for parameters in the recipe file: {}.",
            missing_keys
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    if !extra_keys.is_empty() {
        message.push_str(&format!(
            "\nUnnecessary parameter definitions: {}.",
            extra_keys
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Err(anyhow::anyhow!("{}", message.trim_end()))
}

fn validate_optional_parameters(parameters: &Option<Vec<RecipeParameter>>) -> Result<()> {
    let empty_params = vec![];
    let params = parameters.as_ref().unwrap_or(&empty_params);

    let file_params_with_defaults: Vec<String> = params
        .iter()
        .filter(|p| matches!(p.input_type, RecipeParameterInputType::File) && p.default.is_some())
        .map(|p| p.key.clone())
        .collect();

    if !file_params_with_defaults.is_empty() {
        return Err(anyhow::anyhow!("File parameters cannot have default values to avoid importing sensitive user files: {}", file_params_with_defaults.join(", ")));
    }

    let optional_params_without_default_values: Vec<String> = params
        .iter()
        .filter(|p| {
            matches!(p.requirement, RecipeParameterRequirement::Optional) && p.default.is_none()
        })
        .map(|p| p.key.clone())
        .collect();

    if optional_params_without_default_values.is_empty() {
        Ok(())
    } else {
        Err(anyhow::anyhow!("Optional parameters missing default values in the recipe: {}. Please provide defaults.", optional_params_without_default_values.join(", ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_recipe_template_from_content_success() {
        let recipe_content = r#"
version: 1.0.0
title: Test Recipe
description: A test recipe for validation
instructions: Test instructions with {{ user_role }}
prompt: |
  {% if user_role in ["Director, Account Management", "Senior Director, Account Management"] %}
  - Focus on strategic planning and organizational performance
  {% else %}
  - Provide foundational account management guidance
  {% endif %}
parameters:
  - key: user_role
    input_type: string
    requirement: required
    description: A test parameter
"#;

        let result = validate_recipe_template_from_content(recipe_content, None);
        if let Err(e) = &result {
            eprintln!("Validation error: {}", e);
            eprintln!("Error chain:");
            let mut source = e.source();
            while let Some(err) = source {
                eprintln!("  Caused by: {}", err);
                source = err.source();
            }
        }
        assert!(result.is_ok(), "Validation failed: {:?}", result.err());

        let recipe = result.unwrap();
        assert_eq!(recipe.title, "Test Recipe");
        assert_eq!(recipe.description, "A test recipe for validation");
        assert!(recipe.instructions.is_some());
        println!("Recipe: {:?}", recipe.prompt);
    }
}
