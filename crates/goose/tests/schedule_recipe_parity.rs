use goose::recipe::validate_recipe::{validate_recipe_for_scheduling, RecipeFileFormat};

#[test]
fn checked_in_recipes_pass_scheduling_validation() {
    let root = env!("CARGO_MANIFEST_DIR").replace("/crates/goose", "");
    let mut paths = vec![format!(
        "{root}/documentation/src/pages/recipes/data/recipes/technical-debt-tracker.yaml"
    )];
    for name in [
        "duplication-detection",
        "complexity-analysis",
        "dependency-analysis",
        "test-coverage-analysis",
    ] {
        paths.push(format!(
            "{root}/documentation/src/pages/recipes/data/recipes/subrecipes/{name}.yaml"
        ));
    }

    for path in &paths {
        let content = std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("failed to read {path}: {error}"));
        validate_recipe_for_scheduling(&content, None, RecipeFileFormat::Yaml).unwrap_or_else(
            |error| {
                panic!(
                    "{path} passes `goose recipe validate`, so scheduling must accept it too: {error}"
                )
            },
        );
    }
}
