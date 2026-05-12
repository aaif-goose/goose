use console::style;
use serde_json::Value;
use std::collections::HashMap;
use std::io::{self, BufRead, IsTerminal, Write};

pub fn collect_elicitation_input(
    message: &str,
    schema: &Value,
) -> io::Result<Option<HashMap<String, Value>>> {
    if !message.is_empty() {
        println!("\n{}", style(message).cyan());
    }

    let properties = schema.get("properties").and_then(|p| p.as_object());

    // Schema-less (or empty-schema) elicitations are pure approval prompts —
    // offer an explicit Y/N confirmation instead of silently auto-accepting.
    let properties = match properties {
        Some(props) if !props.is_empty() => props,
        _ => {
            let prompt = if message.is_empty() {
                "Approve this action?"
            } else {
                "Approve?"
            };
            return match cliclack::confirm(prompt).initial_value(true).interact() {
                Ok(true) => Ok(Some(HashMap::new())),
                Ok(false) => Ok(None),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => Ok(None),
                Err(e) => Err(e),
            };
        }
    };

    let required: Vec<&str> = schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    let mut data: HashMap<String, Value> = HashMap::new();

    for (name, field_schema) in properties {
        let is_required = required.contains(&name.as_str());
        let field_type = field_schema
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("string");
        let description = field_schema.get("description").and_then(|d| d.as_str());
        let default = field_schema.get("default");
        let enum_values = field_schema.get("enum").and_then(|e| e.as_array());

        // makes a little true/false toggle
        if field_type == "boolean" {
            let label = match description {
                Some(desc) => format!("{} ({})", name, desc),
                None => name.clone(),
            };
            let default_bool = default.and_then(|v| v.as_bool()).unwrap_or(false);

            match cliclack::confirm(&label)
                .initial_value(default_bool)
                .interact()
            {
                Ok(v) => {
                    data.insert(name.clone(), Value::Bool(v));
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => return Ok(None),
                Err(e) => return Err(e),
            }
            continue;
        }

        if let Some(options) = enum_values {
            let opts: Vec<&str> = options.iter().filter_map(|v| v.as_str()).collect();
            println!("  {}: {}", style("Options").dim(), opts.join(", "));
        }

        print!("{}", style(name).yellow());
        if let Some(desc) = description {
            print!(" {}", style(format!("({})", desc)).dim());
        }
        if is_required {
            print!("{}", style("*").red());
        }
        if let Some(def) = default {
            print!(" {}", style(format!("[{}]", format_default(def))).dim());
        }
        print!(": ");
        io::stdout().flush()?;

        let input = read_line()?;

        // Handle Ctrl+C / EOF for cancellation
        if input.is_none() {
            return Ok(None);
        }
        let input = input.unwrap();

        let value = if input.is_empty() {
            default.cloned()
        } else {
            Some(parse_value(&input, field_type, enum_values))
        };

        if let Some(v) = value {
            if !v.is_null() {
                data.insert(name.clone(), v);
            }
        }

        if is_required && !data.contains_key(name) {
            println!(
                "{}",
                style(format!("Required field '{}' is missing", name)).red()
            );
            return Ok(None);
        }
    }

    println!();
    Ok(Some(data))
}

fn read_line() -> io::Result<Option<String>> {
    if !std::io::stdin().is_terminal() {
        let mut line = String::new();
        io::stdin().lock().read_line(&mut line)?;
        return Ok(Some(line.trim().to_string()));
    }

    let mut line = String::new();
    match io::stdin().lock().read_line(&mut line) {
        Ok(0) => Ok(None), // EOF
        Ok(_) => Ok(Some(line.trim().to_string())),
        Err(e) if e.kind() == io::ErrorKind::Interrupted => Ok(None),
        Err(e) => Err(e),
    }
}

fn format_default(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        _ => value.to_string(),
    }
}

fn parse_value(input: &str, field_type: &str, enum_values: Option<&Vec<Value>>) -> Value {
    if let Some(options) = enum_values {
        let valid: Vec<&str> = options.iter().filter_map(|v| v.as_str()).collect();
        if valid.contains(&input) {
            return Value::String(input.to_string());
        }
        if let Ok(idx) = input.parse::<usize>() {
            if idx > 0 && idx <= valid.len() {
                return Value::String(valid[idx - 1].to_string());
            }
        }
    }

    match field_type {
        "boolean" => {
            let lower = input.to_lowercase();
            Value::Bool(matches!(lower.as_str(), "true" | "yes" | "y" | "1"))
        }
        "integer" => input
            .parse::<i64>()
            .map(|n| Value::Number(n.into()))
            .unwrap_or(Value::Null),
        "number" => input
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        _ => Value::String(input.to_string()),
    }
}
