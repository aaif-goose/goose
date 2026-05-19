use anyhow::Result;
use console::{measure_text_width, Term};
use goose::skills::list_installed_skills;

const SEPARATOR: &str = " - ";

pub fn handle_skills_list() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let terminal_width = terminal_width();
    let mut skills = list_installed_skills(Some(&cwd));
    skills.sort_by(|a, b| a.name.cmp(&b.name));

    for skill in skills {
        println!(
            "{}",
            skill_line(&skill.name, &skill.description, terminal_width)
        );
    }

    Ok(())
}

fn terminal_width() -> Option<usize> {
    Term::stdout()
        .size_checked()
        .map(|(_height, width)| width as usize)
}

fn skill_line(name: &str, description: &str, max_display_width: Option<usize>) -> String {
    let line = format!("{name}{SEPARATOR}{description}");

    match max_display_width {
        Some(width) => truncate_to_display_width(&line, width),
        None => line,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_line_respects_terminal_width() {
        assert_eq!(skill_line("name", "abcdef", Some(10)), "name - ...");
    }

    #[test]
    fn skill_line_handles_zero_width() {
        assert_eq!(skill_line("name", "abcdef", Some(0)), "");
    }
}
