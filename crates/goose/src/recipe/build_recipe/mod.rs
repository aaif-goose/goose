use crate::recipe::read_recipe_file_content::read_parameter_file_content;
use crate::recipe::template_recipe::{
    render_recipe_content_with_params, render_recipe_content_with_structured_params,
};
use crate::recipe::validate_recipe::validate_recipe_template_from_content;
use crate::recipe::{
    Recipe, RecipeParameter, RecipeParameterInputType, RecipeParameterRequirement,
    BUILT_IN_RECIPE_DIR_PARAM,
};
use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum RecipeError {
    #[error("Missing required parameters: {parameters:?}")]
    MissingParams { parameters: Vec<String> },
    #[error("Invalid recipe: {source}")]
    Invalid { source: anyhow::Error },
}

fn render_recipe_template<F>(
    recipe_content: String,
    recipe_dir: &Path,
    params: Vec<(String, String)>,
    user_prompt_fn: Option<F>,
) -> Result<(String, Vec<String>)>
where
    F: Fn(&str, &str) -> Result<String, anyhow::Error>,
{
    let recipe_dir_str = recipe_dir.display().to_string();

    let recipe_parameters =
        validate_recipe_template_from_content(&recipe_content, Some(recipe_dir_str.clone()))?
            .parameters;

    let has_structured_params = recipe_parameters.as_ref().is_some_and(|params| {
        params.iter().any(|p| {
            matches!(
                p.input_type,
                RecipeParameterInputType::Object | RecipeParameterInputType::Array
            )
        })
    });

    let (params_for_template, missing_params) = apply_values_to_parameters(
        &params,
        recipe_parameters.clone(),
        &recipe_dir_str,
        user_prompt_fn,
    )?;

    let rendered_content = if missing_params.is_empty() {
        if has_structured_params {
            let structured_map =
                to_structured_params(&params_for_template, recipe_parameters.as_deref())?;
            render_recipe_content_with_structured_params(&recipe_content, &structured_map)?
        } else {
            render_recipe_content_with_params(&recipe_content, &params_for_template)?
        }
    } else {
        String::new()
    };

    Ok((rendered_content, missing_params))
}

fn to_structured_params(
    string_map: &HashMap<String, String>,
    recipe_parameters: Option<&[RecipeParameter]>,
) -> Result<HashMap<String, Value>> {
    let structured_keys: HashMap<&str, &RecipeParameterInputType> = recipe_parameters
        .unwrap_or_default()
        .iter()
        .filter(|p| {
            matches!(
                p.input_type,
                RecipeParameterInputType::Object | RecipeParameterInputType::Array
            )
        })
        .map(|p| (p.key.as_str(), &p.input_type))
        .collect();

    string_map
        .iter()
        .map(|(k, v)| {
            let value = if let Some(input_type) = structured_keys.get(k.as_str()) {
                let parsed: Value = serde_json::from_str(v).map_err(|e| {
                    anyhow::anyhow!(
                        "Parameter '{}' has input_type {} but value is not valid JSON: {}",
                        k,
                        input_type,
                        e
                    )
                })?;
                match input_type {
                    RecipeParameterInputType::Object if !parsed.is_object() => {
                        anyhow::bail!(
                            "Parameter '{}' has input_type object but received {}",
                            k,
                            json_type_name(&parsed)
                        )
                    }
                    RecipeParameterInputType::Array if !parsed.is_array() => {
                        anyhow::bail!(
                            "Parameter '{}' has input_type array but received {}",
                            k,
                            json_type_name(&parsed)
                        )
                    }
                    _ => parsed,
                }
            } else {
                Value::String(v.clone())
            };
            Ok((k.clone(), value))
        })
        .collect()
}

fn json_type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

pub fn build_recipe_from_template<F>(
    recipe_content: String,
    recipe_dir: &Path,
    params: Vec<(String, String)>,
    user_prompt_fn: Option<F>,
) -> Result<Recipe, RecipeError>
where
    F: Fn(&str, &str) -> Result<String, anyhow::Error>,
{
    let (rendered_content, missing_params) =
        render_recipe_template(recipe_content, recipe_dir, params.clone(), user_prompt_fn)
            .map_err(|source| RecipeError::Invalid { source })?;

    if !missing_params.is_empty() {
        return Err(RecipeError::MissingParams {
            parameters: missing_params,
        });
    }

    let mut recipe = Recipe::from_content(&rendered_content)
        .map_err(|source| RecipeError::Invalid { source })?;

    if let Some(ref mut sub_recipes) = recipe.sub_recipes {
        for sub_recipe in sub_recipes {
            sub_recipe.path = resolve_sub_recipe_path(&sub_recipe.path, recipe_dir)?;
        }
    }

    Ok(recipe)
}

pub fn build_recipe_from_template_with_positional_params<F>(
    recipe_content: String,
    recipe_dir: &Path,
    params: Vec<String>,
    user_prompt_fn: Option<F>,
) -> Result<Recipe, RecipeError>
where
    F: Fn(&str, &str) -> Result<String, anyhow::Error>,
{
    let recipe_dir_str = recipe_dir.display().to_string();

    let recipe_parameters =
        validate_recipe_template_from_content(&recipe_content, Some(recipe_dir_str.clone()))
            .map_err(|source| RecipeError::Invalid { source })?
            .parameters;

    let param_pairs: Vec<(String, String)> = if let Some(recipe_params) = &recipe_parameters {
        let required_count = recipe_params.iter().filter(|p| p.default.is_none()).count();
        if params.len() < required_count {
            let required_keys: Vec<String> = recipe_params
                .iter()
                .filter(|p| p.default.is_none())
                .map(|p| p.key.clone())
                .collect();
            return Err(RecipeError::MissingParams {
                parameters: required_keys,
            });
        }
        recipe_params
            .iter()
            .zip(params.iter())
            .map(|(rp, p)| (rp.key.clone(), p.clone()))
            .collect()
    } else {
        vec![]
    };

    build_recipe_from_template(recipe_content, recipe_dir, param_pairs, user_prompt_fn)
}

pub fn apply_values_to_parameters<F>(
    user_params: &[(String, String)],
    recipe_parameters: Option<Vec<RecipeParameter>>,
    recipe_dir: &str,
    user_prompt_fn: Option<F>,
) -> Result<(HashMap<String, String>, Vec<String>)>
where
    F: Fn(&str, &str) -> Result<String, anyhow::Error>,
{
    let mut param_map: HashMap<String, String> = user_params.iter().cloned().collect();
    param_map.insert(
        BUILT_IN_RECIPE_DIR_PARAM.to_string(),
        recipe_dir.to_string(),
    );
    let mut missing_params: Vec<String> = Vec::new();
    for param in recipe_parameters.unwrap_or_default() {
        if !param_map.contains_key(&param.key) {
            match (&param.default, &param.requirement) {
                (Some(default), _) => param_map.insert(param.key.clone(), default.clone()),
                (None, RecipeParameterRequirement::UserPrompt) if user_prompt_fn.is_some() => {
                    let input_value =
                        user_prompt_fn.as_ref().unwrap()(&param.key, &param.description)?;
                    param_map.insert(param.key.clone(), input_value)
                }
                _ => {
                    missing_params.push(param.key.clone());
                    None
                }
            };
        } else if matches!(param.input_type, RecipeParameterInputType::File) {
            let file_path = param_map.get(&param.key).unwrap();
            let file_content = read_parameter_file_content(file_path)?;
            param_map.insert(param.key.clone(), file_content);
        }
    }
    Ok((param_map, missing_params))
}

pub fn resolve_sub_recipe_path(
    sub_recipe_path: &str,
    parent_recipe_dir: &Path,
) -> Result<String, RecipeError> {
    let path = if Path::new(sub_recipe_path).is_absolute() {
        Path::new(sub_recipe_path).to_path_buf()
    } else {
        parent_recipe_dir.join(sub_recipe_path)
    };
    if !path.exists() {
        return Err(RecipeError::Invalid {
            source: anyhow::anyhow!("Sub-recipe file does not exist: {}", path.display()),
        });
    }

    Ok(path.display().to_string())
}

#[cfg(test)]
mod tests;
