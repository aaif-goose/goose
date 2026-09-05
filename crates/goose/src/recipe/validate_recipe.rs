use crate::recipe::read_recipe_file_content::RecipeFile;
use crate::recipe::template_recipe::{parse_recipe_content, prepare_recipe_template};
use crate::recipe::{
    Recipe, RecipeParameter, RecipeParameterInputType, RecipeParameterRequirement,
    BUILT_IN_RECIPE_DIR_PARAM,
};
use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecipeFileFormat {
    Json,
    Yaml,
}

pub fn recipe_file_format(path: &Path) -> RecipeFileFormat {
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
    {
        RecipeFileFormat::Json
    } else {
        RecipeFileFormat::Yaml
    }
}

/// Safe diagnostic contract for scheduling paths (`manage_schedule`, `schedule add`).
///
/// `GenericParse` covers template and syntax failures and never reflects file
/// contents. `InvalidSchema` messages are derived from the recipe schema or
/// parameter positions only, never from offending values in the file.
#[derive(Debug)]
pub enum SchedulerRecipeError {
    GenericParse(RecipeFileFormat),
    InvalidSchema(String),
}

impl std::fmt::Display for SchedulerRecipeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SchedulerRecipeError::GenericParse(RecipeFileFormat::Json) => {
                write!(f, "Invalid JSON recipe")
            }
            SchedulerRecipeError::GenericParse(RecipeFileFormat::Yaml) => {
                write!(f, "Invalid YAML recipe")
            }
            SchedulerRecipeError::InvalidSchema(message) => write!(f, "Invalid recipe: {message}"),
        }
    }
}

impl std::error::Error for SchedulerRecipeError {}

/// Parse once and validate for the scheduling paths, returning either a
/// validated `Recipe` or a safe error that callers propagate unchanged.
pub fn validate_recipe_for_scheduling(
    content: &str,
    recipe_dir: Option<String>,
    format: RecipeFileFormat,
) -> Result<Recipe, SchedulerRecipeError> {
    let (rendered_content, template_variables) = prepare_recipe_template(content, recipe_dir)
        .map_err(|_| SchedulerRecipeError::GenericParse(format))?;

    let document: serde_yaml::Value = serde_yaml::from_str(&rendered_content)
        .map_err(|_| SchedulerRecipeError::GenericParse(format))?;

    let mut recipe: Recipe = match document.get("recipe") {
        Some(nested_recipe) => convert_recipe_from_value(nested_recipe.clone(), format)?,
        None => convert_recipe_from_str(&rendered_content, format)?,
    };

    recipe.ensure_analyze_for_developer();
    recipe.ensure_summon_for_subrecipes();

    validate_prompt_or_instructions(&recipe)
        .map_err(|error| SchedulerRecipeError::InvalidSchema(error.to_string()))?;
    validate_retry_config(&recipe)
        .map_err(|error| SchedulerRecipeError::InvalidSchema(error.to_string()))?;
    validate_scheduling_parameters(&recipe.parameters, &template_variables)
        .map_err(SchedulerRecipeError::InvalidSchema)?;
    if let Some(response) = &recipe.response {
        if let Some(json_schema) = &response.json_schema {
            validate_json_schema(json_schema).map_err(|error| {
                match error.to_string().as_str() {
                    "JSON schema must be an object" | "Empty JSON schema is not allowed" => {
                        SchedulerRecipeError::InvalidSchema(error.to_string())
                    }
                    _ => SchedulerRecipeError::InvalidSchema(
                        "invalid response.json_schema".to_string(),
                    ),
                }
            })?;
        }
    }

    Ok(recipe)
}

fn convert_recipe_from_value(
    value: serde_yaml::Value,
    format: RecipeFileFormat,
) -> Result<Recipe, SchedulerRecipeError> {
    serde_path_to_error::deserialize(value)
        .map_err(|error| classify_conversion_error(&error, format))
}

fn convert_recipe_from_str(
    content: &str,
    format: RecipeFileFormat,
) -> Result<Recipe, SchedulerRecipeError> {
    serde_path_to_error::deserialize(serde_yaml::Deserializer::from_str(content))
        .map_err(|error| classify_conversion_error(&error, format))
}

fn classify_conversion_error(
    error: &serde_path_to_error::Error<serde_yaml::Error>,
    format: RecipeFileFormat,
) -> SchedulerRecipeError {
    let path = error.path().to_string();
    let path = if path == "." { String::new() } else { path };

    let mut message = crate::recipe::strip_error_location(&error.inner().to_string());
    if !path.is_empty() {
        if let Some(stripped) = message
            .strip_prefix(&path)
            .and_then(|rest| rest.strip_prefix(": "))
        {
            message = stripped.to_string();
        }
    }

    if !is_schema_conversion_message(&message) {
        return SchedulerRecipeError::GenericParse(format);
    }

    let missing_or_duplicate =
        message.starts_with("missing field `") || message.starts_with("duplicate field `");

    match path.as_str() {
        "" => {
            if missing_or_duplicate && message.ends_with('`') {
                SchedulerRecipeError::InvalidSchema(message)
            } else {
                SchedulerRecipeError::GenericParse(format)
            }
        }
        _ if !schema_safe_path(&path) => SchedulerRecipeError::GenericParse(format),
        _ if missing_or_duplicate => {
            SchedulerRecipeError::InvalidSchema(format!("{path}: {message}"))
        }
        _ => SchedulerRecipeError::InvalidSchema(format!("{path} is invalid")),
    }
}

const RECIPE_PARAMETER_FIELDS: &[&str] = &[
    "key",
    "input_type",
    "requirement",
    "description",
    "default",
    "options",
];

/// Paths may only name schema fields and numeric indices. serde_path_to_error
/// also embeds map keys from the recipe file, so anything else (for example a
/// key under `extensions[*].envs`) is file content and must never be surfaced.
fn schema_safe_path(path: &str) -> bool {
    fn is_parameter_field(segment: &str) -> bool {
        RECIPE_PARAMETER_FIELDS.contains(&schema_identifier(segment).unwrap_or_default())
    }

    let mut segments = path.split('.');
    let Some(root) = segments.next() else {
        return false;
    };
    if schema_identifier(root) != Some("parameters") {
        return false;
    }

    match segments.next() {
        None => true,
        Some(second) if root.contains('[') => {
            is_parameter_field(second) && segments.next().is_none()
        }
        Some(item) => {
            if !(item.contains('[') && schema_identifier(item) == Some("parameters")) {
                return false;
            }
            match segments.next() {
                None => true,
                Some(field) => is_parameter_field(field) && segments.next().is_none(),
            }
        }
    }
}

/// Returns the base identifier of a segment iff every `[...]` it carries is a
/// bare numeric index; `None` when the segment contains other content.
fn schema_identifier(segment: &str) -> Option<&str> {
    let mut pieces = segment.split('[');
    let base = pieces.next()?;
    if base.is_empty() || !base.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return None;
    }
    for group in pieces {
        let digits = group.strip_suffix(']')?;
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
    }
    Some(base)
}

fn is_schema_conversion_message(message: &str) -> bool {
    [
        "missing field `",
        "duplicate field `",
        "invalid type:",
        "unknown variant `",
        "unknown field `",
        "invalid length:",
    ]
    .iter()
    .any(|prefix| message.starts_with(prefix))
}

fn format_parameter_paths(indices: &[usize]) -> String {
    indices
        .iter()
        .map(|index| format!("parameters[{index}]"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn validate_scheduling_parameters(
    parameters: &Option<Vec<RecipeParameter>>,
    template_variables: &HashSet<String>,
) -> Result<(), String> {
    let params = parameters.as_deref().unwrap_or_default();

    let file_defaults: Vec<usize> = params
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            matches!(p.input_type, RecipeParameterInputType::File) && p.default.is_some()
        })
        .map(|(index, _)| index)
        .collect();
    if !file_defaults.is_empty() {
        return Err(format!(
            "file parameters cannot have default values at {}",
            format_parameter_paths(&file_defaults)
        ));
    }

    let optional_without_default: Vec<usize> = params
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            matches!(p.requirement, RecipeParameterRequirement::Optional) && p.default.is_none()
        })
        .map(|(index, _)| index)
        .collect();
    if !optional_without_default.is_empty() {
        return Err(format!(
            "optional parameters require default values at {}",
            format_parameter_paths(&optional_without_default)
        ));
    }

    let mut defined_keys = HashSet::new();
    for (index, parameter) in params.iter().enumerate() {
        if !defined_keys.insert(parameter.key.clone()) {
            return Err(format!(
                "duplicate parameter definition at parameters[{index}]"
            ));
        }
    }

    let mut referenced_keys = template_variables.clone();
    referenced_keys.remove(BUILT_IN_RECIPE_DIR_PARAM);

    let undefined_count = referenced_keys.difference(&defined_keys).count();
    if undefined_count > 0 {
        return Err(format!(
            "missing parameter definitions for {undefined_count} template variables"
        ));
    }

    let unnecessary_indices: Vec<usize> = params
        .iter()
        .enumerate()
        .filter(|(_, p)| !referenced_keys.contains(&p.key))
        .map(|(index, _)| index)
        .collect();
    if !unnecessary_indices.is_empty() {
        return Err(format!(
            "unnecessary parameter definitions at {}",
            format_parameter_paths(&unnecessary_indices)
        ));
    }

    Ok(())
}

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
    let schema_object = schema
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("JSON schema must be an object"))?;
    if schema_object.is_empty() {
        return Err(anyhow::anyhow!("Empty JSON schema is not allowed"));
    }
    jsonschema::validator_for(schema)
        .map(|_| ())
        .map_err(|error| anyhow::anyhow!("JSON schema validation failed: {error}"))
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
    let recipe = parse_and_validate_parameters(recipe_content, recipe_dir)?;

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

    let mut param_keys = HashSet::new();
    for parameter in recipe_parameters.as_deref().unwrap_or_default() {
        if !param_keys.insert(parameter.key.clone()) {
            return Err(anyhow::anyhow!(
                "Duplicate parameter definition: {}.",
                parameter.key
            ));
        }
    }

    let missing_keys = template_variables
        .difference(&param_keys)
        .collect::<Vec<_>>();

    let extra_keys = param_keys
        .difference(&template_variables)
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

    fn recipe_with_duplicate_parameter_keys(parameters: &str) -> String {
        format!(
            r#"
version: 1.0.0
title: Duplicate parameters
description: Duplicate parameter validation
instructions: Test {{{{ value }}}}
parameters:
{parameters}
"#
        )
    }

    #[test]
    fn test_rejects_string_then_file_parameter_with_same_key() {
        let recipe_content = recipe_with_duplicate_parameter_keys(
            r#"  - key: value
    input_type: string
    requirement: optional
    default: file.txt
    description: A string parameter
  - key: value
    input_type: file
    requirement: required
    description: A file parameter"#,
        );

        let error = validate_recipe_template_from_content(&recipe_content, None).unwrap_err();

        assert_eq!(error.to_string(), "Duplicate parameter definition: value.");
    }

    #[test]
    fn test_rejects_file_then_string_parameter_with_same_key() {
        let recipe_content = recipe_with_duplicate_parameter_keys(
            r#"  - key: value
    input_type: file
    requirement: required
    description: A file parameter
  - key: value
    input_type: string
    requirement: optional
    default: file.txt
    description: A string parameter"#,
        );

        let error = validate_recipe_template_from_content(&recipe_content, None).unwrap_err();

        assert_eq!(error.to_string(), "Duplicate parameter definition: value.");
    }

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

    fn scheduling_error(content: &str) -> SchedulerRecipeError {
        validate_recipe_for_scheduling(content, None, RecipeFileFormat::Yaml).unwrap_err()
    }

    #[test]
    fn scheduling_accepts_valid_recipe() {
        let recipe = validate_recipe_for_scheduling(
            "title: Valid\ndescription: hi\nprompt: Run\n",
            None,
            RecipeFileFormat::Yaml,
        )
        .unwrap();
        assert_eq!(recipe.title, "Valid");
    }

    #[test]
    fn scheduling_surfaces_missing_required_fields() {
        let error = scheduling_error("description: Missing title\nprompt: Run safely\n");
        assert_eq!(error.to_string(), "Invalid recipe: missing field `title`");
    }

    #[test]
    fn scheduling_reports_parameter_field_paths() {
        let error = scheduling_error(
            "title: Test\ndescription: hi\nprompt: Run {{ a }}\nparameters:\n  - key: a\n    input_type: string\n    requirement: required\n",
        );
        assert_eq!(
            error.to_string(),
            "Invalid recipe: parameters[0]: missing field `description`"
        );

        let secret = "yaml-secret-242";
        let error = scheduling_error(&format!(
            "title: Test\ndescription: hi\nprompt: Run {{ a }}\nparameters:\n  - key: a\n    input_type: {secret}\n    requirement: required\n    description: hi\n"
        ));
        assert_eq!(
            error.to_string(),
            "Invalid recipe: parameters[0].input_type is invalid"
        );
        assert!(!error.to_string().contains(secret));
    }

    #[test]
    fn scheduling_accepts_unquoted_numeric_default_like_runtime() {
        let recipe = validate_recipe_for_scheduling(
            "title: Test\ndescription: hi\nprompt: Run {{ count }}\nparameters:\n  - key: count\n    input_type: string\n    requirement: optional\n    default: 15\n    description: hi\n",
            None,
            RecipeFileFormat::Yaml,
        )
        .unwrap();
        assert_eq!(
            recipe.parameters.as_ref().unwrap()[0].default.as_deref(),
            Some("15")
        );
    }

    #[test]
    fn scheduling_rejects_duplicate_fields_like_runtime_load() {
        let error = scheduling_error("title: First\ntitle: Second\ndescription: hi\nprompt: Run\n");
        assert_eq!(error.to_string(), "Invalid YAML recipe");
    }

    #[test]
    fn scheduling_sanitizes_value_echoing_conversion_errors() {
        let error = scheduling_error("missing field yaml-secret-242\n");
        assert_eq!(error.to_string(), "Invalid YAML recipe");

        let error = scheduling_error(
            "title: Test\ndescription: hi\nprompt: hi\nparameters:\n  - key: foo\n    input_type: yaml-secret-242\n    requirement: required\n    description: hi\n",
        );
        assert_eq!(
            error.to_string(),
            "Invalid recipe: parameters[0].input_type is invalid"
        );
        assert!(!error.to_string().contains("yaml-secret-242"));
    }

    #[test]
    fn scheduling_redacts_file_controlled_map_keys_from_paths() {
        let secret = "yaml-secret-242";
        let error = scheduling_error(&format!(
            "title: Test\ndescription: hi\nprompt: hi\nextensions:\n  - type: stdio\n    name: ext\n    cmd: run\n    args: []\n    envs:\n      {secret}: []\n"
        ));
        assert_eq!(error.to_string(), "Invalid YAML recipe");
        assert!(!error.to_string().contains(secret));
    }

    #[test]
    fn schema_safe_path_accepts_only_schema_fields_and_indices() {
        use super::schema_safe_path;
        assert!(schema_safe_path("parameters"));
        assert!(schema_safe_path("parameters[0]"));
        assert!(schema_safe_path("parameters[2].input_type"));
        assert!(schema_safe_path("parameters[1].options[3]"));
        assert!(!schema_safe_path("title"));
        assert!(!schema_safe_path("parameters[0].envs.secret-key"));
        assert!(!schema_safe_path("extensions[0].envs.yaml-secret-242"));
        assert!(!schema_safe_path("parameters[0].unknown_field"));
        assert!(!schema_safe_path("parameters[0][x]"));
        assert!(!schema_safe_path("parameters.a.b"));
    }

    #[test]
    fn scheduling_reports_parameter_positions_without_values() {
        let secret = "yaml-secret-242";
        let error = scheduling_error(&format!(
            "title: Test\ndescription: hi\nprompt: hi\nparameters:\n  - key: {secret}\n    input_type: string\n    requirement: required\n    description: hi\n"
        ));
        assert_eq!(
            error.to_string(),
            "Invalid recipe: unnecessary parameter definitions at parameters[0]"
        );
        assert!(!error.to_string().contains(secret));
    }

    #[test]
    fn scheduling_rejects_duplicate_parameters_by_position() {
        let error = scheduling_error(
            "title: Test\ndescription: hi\nprompt: Run {{ a }}\nparameters:\n  - key: a\n    input_type: string\n    requirement: required\n    description: hi\n  - key: a\n    input_type: string\n    requirement: required\n    description: hi\n",
        );
        assert_eq!(
            error.to_string(),
            "Invalid recipe: duplicate parameter definition at parameters[1]"
        );
    }

    #[test]
    fn scheduling_uses_json_format_for_json_recipes() {
        let error = validate_recipe_for_scheduling(
            "{\"description\": \"no title\", \"prompt\": \"Run\"}",
            None,
            RecipeFileFormat::Json,
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "Invalid recipe: missing field `title`");

        let error = validate_recipe_for_scheduling(
            "{\"title\": \"Test\", \"description\": \"hi\", \"prompt\": \"hi\", \"parameters\": [{\"key\": \"foo\", \"input_type\": \"yaml-secret-242\", \"requirement\": \"required\", \"description\": \"hi\"}]}",
            None,
            RecipeFileFormat::Json,
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "Invalid recipe: parameters[0].input_type is invalid"
        );
        assert!(!error.to_string().contains("yaml-secret-242"));
    }

    #[test]
    fn response_json_schema_must_be_an_object() {
        let recipe_content = r#"
version: 1.0.0
title: Boolean schema
description: Boolean schema
instructions: Return structured output
response:
  json_schema: true
"#;

        let error = validate_recipe_template_from_content(recipe_content, None).unwrap_err();

        assert_eq!(error.to_string(), "JSON schema must be an object");
    }

    #[test]
    fn response_json_schema_accepts_an_object_schema() {
        let recipe_content = r#"
version: 1.0.0
title: Object schema
description: Object schema
instructions: Return structured output
response:
  json_schema:
    type: object
    properties:
      result:
        type: string
"#;

        validate_recipe_template_from_content(recipe_content, None).unwrap();
    }

    #[test]
    fn response_json_schema_must_compile() {
        let recipe_content = r#"
version: 1.0.0
title: Invalid pattern
description: Invalid pattern
instructions: Return structured output
response:
  json_schema:
    type: object
    properties:
      result:
        type: string
        pattern: "["
"#;

        let error = validate_recipe_template_from_content(recipe_content, None).unwrap_err();

        assert!(error.to_string().contains("JSON schema validation failed"));
    }
}
