//! Application-independent skill argument substitution and rendering.
//! Native targets also support filesystem discovery under explicit roots.

mod arguments;
#[cfg(not(target_arch = "wasm32"))]
mod discovery;

#[cfg(not(target_arch = "wasm32"))]
pub use discovery::{discover_skills, SkillRoot};

use anyhow::Result;
use arguments::apply_skill_arguments;
use goose_sdk_types::custom_requests::SourceEntry;
use std::path::Path;

fn skill_argument_names(skill: &SourceEntry) -> Vec<String> {
    skill
        .properties
        .get("arguments")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .filter(|name| !name.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Returns skill content with optional argument substitution.
pub fn skill_content_with_args(skill: &SourceEntry, args: Option<&str>) -> Result<String> {
    let content = if let Some(args) = args {
        apply_skill_arguments(&skill.content, args, &skill_argument_names(skill))?
    } else {
        skill.content.clone()
    };

    Ok(content)
}

/// Formats a loaded skill using caller-supplied, postprocessed content.
pub fn loaded_skill_context(skill: &SourceEntry, content: &str) -> String {
    let title = format!("{} ({})", skill.name, skill.source_type);
    let mut output = format!(
        "# Loaded Skill: {title}\n\n{}\n\n## Content\n\n{}\n",
        skill.description, content
    );

    if !skill.supporting_files.is_empty() {
        let skill_dir = Path::new(&skill.path);
        output.push_str(&format!(
            "\n## Supporting Files\n\nSkill directory: {}\n\n\
             Relative paths in this skill resolve from the skill directory. \
             The shell tool runs in the session working directory, so use the \
             resolved path below or `cd` into the skill directory before running \
             supporting scripts.\n\n",
            skill.path
        ));
        for file in &skill.supporting_files {
            if let Ok(relative) = Path::new(file).strip_prefix(skill_dir) {
                let rel_str = relative.to_string_lossy().replace('\\', "/");
                let resolved_path = Path::new(file).to_string_lossy().replace('\\', "/");
                output.push_str(&format!(
                    "- {} → {} (load_skill(name: \"{}/{}\"))\n",
                    rel_str, resolved_path, skill.name, rel_str
                ));
            }
        }
    }

    output
}
