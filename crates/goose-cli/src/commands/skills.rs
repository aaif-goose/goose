use anyhow::Result;
use console::{measure_text_width, Term};
use goose::skills::list_installed_skills;

const SEPARATOR: &str = " | ";
const MIN_DESCRIPTION_WIDTH: usize = 4;

pub fn handle_skills_list() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let terminal_width = terminal_width();
    let mut skills = list_installed_skills(Some(&cwd));
    skills.sort_by(|a, b| a.name.cmp(&b.name));

    let rows = skills
        .iter()
        .map(|skill| (skill.name.as_str(), description_preview(&skill.description)))
        .collect::<Vec<_>>();
    let name_width = name_column_width(&rows, terminal_width);

    for (name, description) in rows {
        println!(
            "{}",
            skill_line(name, &description, name_width, terminal_width)
        );
    }

    Ok(())
}

fn terminal_width() -> Option<usize> {
    Term::stdout()
        .size_checked()
        .map(|(_height, width)| width as usize)
}

fn name_column_width(rows: &[(&str, String)], max_display_width: Option<usize>) -> usize {
    let longest_name = rows
        .iter()
        .map(|(name, _)| measure_text_width(name))
        .max()
        .unwrap_or(0);

    let Some(width) = max_display_width else {
        return longest_name;
    };

    let separator_width = measure_text_width(SEPARATOR);
    let available_width = width.saturating_sub(separator_width);
    let reserved_description_width = if available_width > MIN_DESCRIPTION_WIDTH {
        MIN_DESCRIPTION_WIDTH
    } else {
        0
    };

    longest_name.min(available_width.saturating_sub(reserved_description_width))
}

fn skill_line(
    name: &str,
    description: &str,
    name_width: usize,
    max_display_width: Option<usize>,
) -> String {
    let displayed_name =
        pad_to_display_width(&truncate_to_display_width(name, name_width), name_width);
    let separator_width = measure_text_width(SEPARATOR);
    let description_width =
        max_display_width.map(|width| width.saturating_sub(name_width + separator_width));
    let displayed_description = match description_width {
        Some(width) => truncate_to_display_width(description, width),
        None => description.to_string(),
    };
    let line = format!("{displayed_name}{SEPARATOR}{displayed_description}");

    match max_display_width {
        Some(width) => truncate_to_display_width(&line, width),
        None => line,
    }
}

fn description_preview(description: &str) -> String {
    description.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_to_display_width(text: &str, max_width: usize) -> String {
    if measure_text_width(text) <= max_width {
        return text.to_string();
    }

    if max_width <= 3 {
        return ".".repeat(max_width);
    }

    let mut output = String::new();
    let suffix_width = measure_text_width("...");

    for ch in text.chars() {
        output.push(ch);
        if measure_text_width(&output) + suffix_width > max_width {
            output.pop();
            break;
        }
    }

    output.push_str("...");
    output
}

fn pad_to_display_width(text: &str, width: usize) -> String {
    let padding = width.saturating_sub(measure_text_width(text));
    format!("{}{}", text, " ".repeat(padding))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn description_preview_collapses_whitespace() {
        assert_eq!(description_preview("one\n\n two\tthree"), "one two three");
    }

    #[test]
    fn skill_line_uses_aligned_table_separator() {
        assert_eq!(skill_line("name", "abcdef", 8, None), "name     | abcdef");
    }

    #[test]
    fn skill_line_respects_terminal_width() {
        assert_eq!(skill_line("name", "abcdef", 4, Some(10)), "name | ...");
    }

    #[test]
    fn skill_line_handles_zero_width() {
        assert_eq!(skill_line("name", "abcdef", 0, Some(0)), "");
    }
}
