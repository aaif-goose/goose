use crate::recipes::print_recipe::{
    missing_parameters_command_line, print_recipe_explanation,
    print_required_parameters_for_template,
};
use crate::recipes::search_recipe::load_recipe_file;
use anyhow::Result;
use goose::recipe::build_recipe::{
    apply_values_to_parameters_without_file_expansion, build_recipe_from_template, RecipeError,
};
use goose::recipe::validate_recipe::parse_and_validate_parameters;
use goose::recipe::Recipe;

fn create_user_prompt_callback() -> impl Fn(&str, &str) -> Result<String> {
    |key: &str, description: &str| -> Result<String> {
        let input_value =
            cliclack::input(format!("Please enter {} ({})", key, description)).interact()?;
        Ok(input_value)
    }
}

pub fn load_recipe(recipe_name: &str, params: Vec<(String, String)>) -> Result<Recipe> {
    let recipe_file = load_recipe_file(recipe_name)?;
    let recipe_content = recipe_file.content;
    let recipe_dir = recipe_file.parent_dir;
    match build_recipe_from_template(
        recipe_content,
        &recipe_dir,
        params,
        Some(create_user_prompt_callback()),
    ) {
        Ok(recipe) => Ok(recipe),
        Err(RecipeError::MissingParams { parameters }) => Err(anyhow::anyhow!(
            "Please provide the following parameters in the command line: {}",
            missing_parameters_command_line(parameters)
        )),
        Err(e) => Err(anyhow::anyhow!(e.to_string())),
    }
}

pub fn render_recipe_as_yaml(recipe_name: &str, params: Vec<(String, String)>) -> Result<()> {
    let recipe = load_recipe(recipe_name, params)?;
    match serde_yaml::to_string(&recipe) {
        Ok(yaml_content) => {
            println!("{}", yaml_content);
            Ok(())
        }
        Err(_) => {
            eprintln!("Failed to serialize recipe to YAML");
            std::process::exit(1);
        }
    }
}

pub fn explain_recipe(recipe_name: &str, params: Vec<(String, String)>) -> Result<()> {
    let recipe_file = load_recipe_file(recipe_name)?;
    let recipe_dir_str = recipe_file.parent_dir.display().to_string();
    let recipe_file_content = &recipe_file.content;
    let recipe_template =
        parse_and_validate_parameters(recipe_file_content, Some(recipe_dir_str.clone()))?;
    let recipe_parameters = recipe_template.parameters.clone();

    let (params_for_template, missing_params) = apply_values_to_parameters_without_file_expansion(
        &params,
        recipe_parameters,
        &recipe_dir_str,
        None::<fn(&str, &str) -> Result<String>>,
    )?;
    print_recipe_explanation(&recipe_template);
    print_required_parameters_for_template(params_for_template, missing_params);

    Ok(())
}

#[cfg(test)]
mod tests {
    use goose::recipe::build_recipe::apply_values_to_parameters_without_file_expansion;
    use goose::recipe::{RecipeParameterInputType, RecipeParameterRequirement};

    use crate::recipes::recipe::load_recipe;

    mod load_recipe {
        use super::*;
        #[test]
        fn test_load_recipe_success() {
            let recipe_content = r#"{
                "version": "1.0.0",
                "title": "Test Recipe",
                "description": "A test recipe",
                "instructions": "Test instructions with {{ my_name }}",
                "parameters": [
                    {
                        "key": "my_name",
                        "input_type": "string",
                        "requirement": "required",
                        "description": "A test parameter"
                    }
                ]
            }"#;
            let temp_dir = tempfile::tempdir().unwrap();
            let recipe_path = temp_dir.path().join("test_recipe.json");
            std::fs::write(&recipe_path, recipe_content).unwrap();

            let params = vec![("my_name".to_string(), "value".to_string())];
            let recipe = load_recipe(recipe_path.to_str().unwrap(), params).unwrap();

            assert_eq!(recipe.title, "Test Recipe");
            assert_eq!(recipe.description, "A test recipe");
            assert_eq!(recipe.instructions.unwrap(), "Test instructions with value");
            // Verify parameters match recipe definition
            assert_eq!(recipe.parameters.as_ref().unwrap().len(), 1);
            let param = &recipe.parameters.as_ref().unwrap()[0];
            assert_eq!(param.key, "my_name");
            assert!(matches!(param.input_type, RecipeParameterInputType::String));
            assert!(matches!(
                param.requirement,
                RecipeParameterRequirement::Required
            ));
            assert_eq!(param.description, "A test parameter");
        }
    }

    #[test]
    fn explanation_preserves_file_parameter_path_without_reading_contents() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("does-not-exist.txt");
        let parameters = vec![goose::recipe::RecipeParameter {
            key: "input_file".to_string(),
            input_type: RecipeParameterInputType::File,
            requirement: RecipeParameterRequirement::Required,
            description: "Input file".to_string(),
            default: None,
            options: None,
        }];

        let (values, missing) = apply_values_to_parameters_without_file_expansion(
            &[("input_file".to_string(), file_path.display().to_string())],
            Some(parameters),
            temp_dir.path().to_str().unwrap(),
            None::<fn(&str, &str) -> anyhow::Result<String>>,
        )
        .unwrap();

        assert!(missing.is_empty());
        assert_eq!(
            values.get("input_file"),
            Some(&file_path.display().to_string())
        );
    }
}
