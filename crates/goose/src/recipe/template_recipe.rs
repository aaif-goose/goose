use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use crate::recipe::{Recipe, BUILT_IN_RECIPE_DIR_PARAM};
use anyhow::Result;
use minijinja::{Environment, UndefinedBehavior};
use regex::Regex;
use serde_json::Value;

const CURRENT_TEMPLATE_NAME: &str = "recipe";
const OPEN_BRACE: &str = "{{";
const CLOSE_BRACE: &str = "}}";

fn preprocess_template_variables(content: &str) -> Result<String> {
    let all_template_variables = extract_template_variables(content);
    let complex_template_variables = filter_complex_variables(&all_template_variables);
    let unparsable_template_variables = filter_unparseable_variables(&complex_template_variables)?;
    replace_unparseable_vars_with_raw(content, &unparsable_template_variables)
}

fn extract_template_variables(content: &str) -> Vec<String> {
    let template_var_re = Regex::new(r"\{\{(.*?)\}\}").unwrap();
    template_var_re
        .captures_iter(content)
        .map(|cap| cap[1].to_string())
        .collect()
}

// filter out variables that are not only alphanumeric and underscores
fn filter_complex_variables(template_variables: &[String]) -> Vec<String> {
    let valid_var_re = Regex::new(r"^\s*[a-zA-Z_][a-zA-Z0-9_]*\s*$").unwrap();
    template_variables
        .iter()
        .filter(|var| !valid_var_re.is_match(var))
        .cloned()
        .collect()
}

fn filter_unparseable_variables(template_variables: &[String]) -> Result<Vec<String>> {
    let mut vars_to_convert = Vec::new();

    for var in template_variables {
        let trimmed = var.trim();

        if trimmed.starts_with('\'') || trimmed.starts_with('"') {
            continue;
        }

        let mut env = Environment::new();
        env.set_undefined_behavior(UndefinedBehavior::Lenient);

        let test_template = format!(
            "{open}{content}{close}",
            open = OPEN_BRACE,
            content = var,
            close = CLOSE_BRACE
        );
        if env.template_from_str(&test_template).is_err() {
            vars_to_convert.push(var.clone());
        }
    }

    Ok(vars_to_convert)
}

fn replace_unparseable_vars_with_raw(
    content: &str,
    unparsable_template_variables: &[String],
) -> Result<String> {
    let mut result = content.to_string();

    for var in unparsable_template_variables {
        let pattern = format!(
            "{open}{content}{close}",
            open = OPEN_BRACE,
            content = var,
            close = CLOSE_BRACE
        );
        let replacement = format!(
            "{{% raw %}}{open}{content}{close}{{% endraw %}}",
            open = OPEN_BRACE,
            close = CLOSE_BRACE,
            content = var
        );
        result = result.replace(&pattern, &replacement);
    }

    Ok(result)
}

pub fn render_recipe_content_with_params(
    content: &str,
    params: &HashMap<String, String>,
) -> Result<String> {
    // Pre-process content to replace empty double quotes with single quotes
    // This prevents MiniJinja from escaping "" to "\"\"" which would break YAML parsing
    let re = Regex::new(r#":\s*"""#).unwrap();
    let content_with_empty_quotes_replaced = re.replace_all(content, ": ''");

    // Pre-process template variables to convert invalid variable names to raw content
    let content_with_safe_variables =
        preprocess_template_variables(&content_with_empty_quotes_replaced)?;

    let env = add_template_in_env(
        &content_with_safe_variables,
        params.get(BUILT_IN_RECIPE_DIR_PARAM).cloned(),
        UndefinedBehavior::Strict,
    )?;
    let template = env.get_template(CURRENT_TEMPLATE_NAME).unwrap();
    let rendered_content = template
        .render(params)
        .map_err(|e| anyhow::anyhow!("Failed to render the recipe {}", e))?;
    Ok(rendered_content)
}

/// Renders recipe content with structured parameters (objects, arrays, scalars).
///
/// This is the structured counterpart to `render_recipe_content_with_params`.
/// It accepts `serde_json::Value` parameters, enabling dot-notation access
/// for objects (`{{ signal.namespace }}`) and iteration for arrays
/// (`{% for item in findings %}`).
///
/// Existing scalar string parameters work identically via `Value::String`.
pub fn render_recipe_content_with_structured_params(
    content: &str,
    params: &HashMap<String, Value>,
) -> Result<String> {
    let re = Regex::new(r#":\s*"""#).unwrap();
    let content_with_empty_quotes_replaced = re.replace_all(content, ": ''");

    let content_with_safe_variables =
        preprocess_template_variables(&content_with_empty_quotes_replaced)?;

    let recipe_dir = params
        .get(BUILT_IN_RECIPE_DIR_PARAM)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let env = add_template_in_env(
        &content_with_safe_variables,
        recipe_dir,
        UndefinedBehavior::Strict,
    )?;
    let template = env.get_template(CURRENT_TEMPLATE_NAME).unwrap();
    let rendered_content = template
        .render(params)
        .map_err(|e| anyhow::anyhow!("Failed to render the recipe {}", e))?;
    Ok(rendered_content)
}

fn add_template_in_env(
    content: &str,
    recipe_dir: Option<String>,
    undefined_behavior: UndefinedBehavior,
) -> Result<Environment<'_>> {
    let mut env = minijinja::Environment::new();
    env.set_undefined_behavior(undefined_behavior);

    if let Some(recipe_dir) = recipe_dir {
        env.set_loader(move |name| {
            let path = Path::new(recipe_dir.as_str()).join(name);
            match std::fs::read_to_string(&path) {
                Ok(content) => Ok(Some(content)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    "could not read template",
                )
                .with_source(e)),
            }
        });
    }

    env.add_template(CURRENT_TEMPLATE_NAME, content)?;
    Ok(env)
}

fn get_env_with_template_variables(
    content: &str,
    recipe_dir: Option<String>,
    undefined_behavior: UndefinedBehavior,
) -> Result<(Environment<'_>, HashSet<String>)> {
    let env = add_template_in_env(content, recipe_dir, undefined_behavior)?;
    let template_variables = {
        let template = env.get_template(CURRENT_TEMPLATE_NAME).unwrap();
        let captured = template.render_captured(())?;
        let state = captured.state();
        let mut vars = HashSet::new();
        for (_, tmpl) in state.env().templates() {
            vars.extend(tmpl.undeclared_variables(true));
        }
        vars
    };
    Ok((env, template_variables))
}

fn uses_template_inheritance(content: &str) -> bool {
    let re = Regex::new(r"\{%-?\s*(extends|include)").unwrap();
    re.is_match(content)
}

pub fn parse_recipe_content(
    content: &str,
    recipe_dir: Option<String>,
) -> Result<(Recipe, HashSet<String>)> {
    // Pre-process template variables to handle invalid variable names
    let preprocessed_content = preprocess_template_variables(content)?;

    let (env, template_variables) = get_env_with_template_variables(
        &preprocessed_content,
        recipe_dir,
        UndefinedBehavior::Lenient,
    )?;
    let template = env.get_template(CURRENT_TEMPLATE_NAME).unwrap();

    // Detect if template uses inheritance or includes
    let recipe_content = if uses_template_inheritance(&preprocessed_content) {
        // Must render to resolve inheritance
        template
            .render(())
            .map_err(|e| anyhow::anyhow!("Failed to parse the recipe {}", e))?
    } else {
        // Preserve conditionals and variables as-is
        preprocessed_content
    };

    let recipe = Recipe::from_content(&recipe_content)?;
    // return recipe (without loading any variables) and the variable names that are in the recipe
    Ok((recipe, template_variables))
}

#[cfg(test)]
mod tests {
    mod render_content_with_params_tests {
        use std::collections::HashMap;

        use crate::recipe::template_recipe::render_recipe_content_with_params;

        #[test]
        fn test_render_content_with_params() {
            // Test basic parameter substitution
            let content = "Hello {{ name }}!";
            let params = HashMap::from([
                ("recipe_dir".to_string(), "some_dir".to_string()),
                ("name".to_string(), "World".to_string()),
            ]);
            let result = render_recipe_content_with_params(content, &params).unwrap();
            assert_eq!(result, "Hello World!");

            // Test empty parameter substitution
            let content = "Hello {{ empty }}!";
            let params = HashMap::from([
                ("recipe_dir".to_string(), "some_dir".to_string()),
                ("empty".to_string(), "".to_string()),
            ]);
            let result = render_recipe_content_with_params(content, &params).unwrap();
            assert_eq!(result, "Hello !");

            // Test multiple parameters
            let content = "{{ greeting }} {{ name }}!";
            let params = HashMap::from([
                ("recipe_dir".to_string(), "some_dir".to_string()),
                ("greeting".to_string(), "Hi".to_string()),
                ("name".to_string(), "Alice".to_string()),
            ]);
            let result = render_recipe_content_with_params(content, &params).unwrap();
            assert_eq!(result, "Hi Alice!");

            // Test missing parameter results in error
            let content = "Hello {{ missing }}!";
            let params = HashMap::from([("recipe_dir".to_string(), "some_dir".to_string())]);
            let err = render_recipe_content_with_params(content, &params).unwrap_err();
            let error_msg = err.to_string();
            assert!(error_msg.contains("Failed to render the recipe"));

            // Test invalid template syntax results in error
            let content = "Hello {{ unclosed";
            let params = HashMap::from([("recipe_dir".to_string(), "some_dir".to_string())]);
            let err = render_recipe_content_with_params(content, &params).unwrap_err();
            assert!(err.to_string().contains("unexpected end of input"));
        }

        #[test]
        fn test_render_content_with_spaced_variables() {
            let content = "Hello {{hf model org}}_{{hf model name}}!";
            let params = HashMap::from([("recipe_dir".to_string(), "some_dir".to_string())]);
            let result = render_recipe_content_with_params(content, &params).unwrap();
            assert_eq!(result, "Hello {{hf model org}}_{{hf model name}}!");

            let content = "Hello {{hf model org}_{hf model name}}!";
            let params = HashMap::from([("recipe_dir".to_string(), "some_dir".to_string())]);
            let result = render_recipe_content_with_params(content, &params).unwrap();
            assert_eq!(result, "Hello {{hf model org}_{hf model name}}!");

            let content = "Hello {{valid_var}}!";
            let params = HashMap::from([
                ("recipe_dir".to_string(), "some_dir".to_string()),
                ("valid_var".to_string(), "World".to_string()),
            ]);
            let result = render_recipe_content_with_params(content, &params).unwrap();
            assert_eq!(result, "Hello World!");

            let content = "{{valid_var}} and {{invalid var}}";
            let params = HashMap::from([
                ("recipe_dir".to_string(), "some_dir".to_string()),
                ("valid_var".to_string(), "Hello".to_string()),
            ]);
            let result = render_recipe_content_with_params(content, &params).unwrap();
            assert_eq!(result, "Hello and {{invalid var}}");
        }

        #[test]
        fn test_empty_prompt() {
            let content = r#"
prompt: ""
name: "Simple Recipe"
description: "A test recipe"
"#;
            let params = HashMap::from([("recipe_dir".to_string(), "test_dir".to_string())]);
            let result = render_recipe_content_with_params(content, &params).unwrap();

            assert!(result.contains("prompt: ''"));
            assert!(!result.contains(r#"prompt: "\"\"""#)); // Should not contain escaped quotes

            assert!(result.contains(r#"name: "Simple Recipe""#));
        }

        #[test]
        fn test_jinja_escape_syntax() {
            let content = r#"{{'{{param_key}}'}}"#;
            let params = HashMap::from([("recipe_dir".to_string(), "test_dir".to_string())]);
            let result = render_recipe_content_with_params(content, &params).unwrap();
            assert_eq!(result, "{{param_key}}");
        }
    }

    mod render_structured_params_tests {
        use std::collections::HashMap;

        use crate::recipe::template_recipe::render_recipe_content_with_structured_params;
        use serde_json::{json, Value};

        fn str_val(s: &str) -> Value {
            Value::String(s.to_string())
        }

        #[test]
        fn test_render_with_string_values_unchanged() {
            let content = "Hello {{ name }}!";
            let params = HashMap::from([
                ("recipe_dir".to_string(), str_val("some_dir")),
                ("name".to_string(), str_val("World")),
            ]);
            let result = render_recipe_content_with_structured_params(content, &params).unwrap();
            assert_eq!(result, "Hello World!");
        }

        #[test]
        fn test_render_with_object_parameter() {
            let content = "Signal: {{ signal.name }} in {{ signal.namespace }}";
            let params = HashMap::from([
                ("recipe_dir".to_string(), str_val("some_dir")),
                (
                    "signal".to_string(),
                    json!({"name": "OOMKilled", "namespace": "production"}),
                ),
            ]);
            let result = render_recipe_content_with_structured_params(content, &params).unwrap();
            assert_eq!(result, "Signal: OOMKilled in production");
        }

        #[test]
        fn test_render_with_object_conditional() {
            let content =
                r#"{% if signal.severity == "critical" %}CRITICAL{% else %}normal{% endif %}"#;
            let params = HashMap::from([
                ("recipe_dir".to_string(), str_val("some_dir")),
                (
                    "signal".to_string(),
                    json!({"severity": "critical", "name": "OOMKilled"}),
                ),
            ]);
            let result = render_recipe_content_with_structured_params(content, &params).unwrap();
            assert_eq!(result, "CRITICAL");
        }

        #[test]
        fn test_render_with_array_parameter() {
            let content = "{% for item in findings %}{{ item.name }}: {{ item.output }}
{% endfor %}";
            let params = HashMap::from([
                ("recipe_dir".to_string(), str_val("some_dir")),
                (
                    "findings".to_string(),
                    json!([
                        {"name": "check-1", "output": "passed"},
                        {"name": "check-2", "output": "failed"}
                    ]),
                ),
            ]);
            let result = render_recipe_content_with_structured_params(content, &params).unwrap();
            assert_eq!(
                result,
                "check-1: passed
check-2: failed
"
            );
        }

        #[test]
        fn test_render_with_nested_object_access() {
            let content = "Owner: {{ enrichment.owner_chain }}";
            let params = HashMap::from([
                ("recipe_dir".to_string(), str_val("some_dir")),
                (
                    "enrichment".to_string(),
                    json!({"owner_chain": "Pod > ReplicaSet > Deployment", "labels": {"app": "api"}}),
                ),
            ]);
            let result = render_recipe_content_with_structured_params(content, &params).unwrap();
            assert_eq!(result, "Owner: Pod > ReplicaSet > Deployment");
        }

        #[test]
        fn test_render_with_optional_object_field() {
            let content = "{% if enrichment.owner_chain is defined %}Chain: {{ enrichment.owner_chain }}{% else %}No chain{% endif %}";
            let params = HashMap::from([
                ("recipe_dir".to_string(), str_val("some_dir")),
                ("enrichment".to_string(), json!({})),
            ]);
            let result = render_recipe_content_with_structured_params(content, &params).unwrap();
            assert_eq!(result, "No chain");
        }

        #[test]
        fn test_render_mixed_scalar_and_object_params() {
            let content =
                "Alert {{ alert_name }} for {{ signal.resource_kind }}/{{ signal.resource_name }}";
            let params = HashMap::from([
                ("recipe_dir".to_string(), str_val("some_dir")),
                ("alert_name".to_string(), str_val("HighMemory")),
                (
                    "signal".to_string(),
                    json!({"resource_kind": "Pod", "resource_name": "api-server-abc"}),
                ),
            ]);
            let result = render_recipe_content_with_structured_params(content, &params).unwrap();
            assert_eq!(result, "Alert HighMemory for Pod/api-server-abc");
        }

        #[test]
        fn test_render_with_empty_array_iteration() {
            let content = "Items:{% for item in findings %} {{ item.name }}{% endfor %} done";
            let params = HashMap::from([
                ("recipe_dir".to_string(), str_val("some_dir")),
                ("findings".to_string(), json!([])),
            ]);
            let result = render_recipe_content_with_structured_params(content, &params).unwrap();
            assert_eq!(result, "Items: done");
        }

        #[test]
        fn test_render_with_deeply_nested_object() {
            let content = "Owner: {{ signal.metadata.labels.app }}";
            let params = HashMap::from([
                ("recipe_dir".to_string(), str_val("some_dir")),
                (
                    "signal".to_string(),
                    json!({"metadata": {"labels": {"app": "api-server", "env": "prod"}}}),
                ),
            ]);
            let result = render_recipe_content_with_structured_params(content, &params).unwrap();
            assert_eq!(result, "Owner: api-server");
        }

        #[test]
        fn test_render_dot_access_on_scalar_fails() {
            let content = "Value: {{ name.field }}";
            let params = HashMap::from([
                ("recipe_dir".to_string(), str_val("some_dir")),
                ("name".to_string(), str_val("just-a-string")),
            ]);
            let result = render_recipe_content_with_structured_params(content, &params);
            assert!(result.is_err());
            let err_msg = result.unwrap_err().to_string();
            assert!(
                err_msg.contains("Failed to render"),
                "Expected render failure, got: {}",
                err_msg
            );
        }
    }
}
