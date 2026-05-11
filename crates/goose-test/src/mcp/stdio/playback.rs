use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::process;

use serde_json::Value;

#[derive(Debug, Clone)]
enum StreamType {
    Stdin,
    Stdout,
    Stderr,
}

#[derive(Debug, Clone)]
struct LogEntry {
    stream_type: StreamType,
    content: String,
}

fn parse_log_line(line: &str) -> Option<LogEntry> {
    line.find(": ").and_then(|pos| {
        let (prefix, content) = line.split_at(pos);
        let content = content.get(2..)?; // Skip ": "

        let stream_type = match prefix {
            "STDIN" => StreamType::Stdin,
            "STDOUT" => StreamType::Stdout,
            "STDERR" => StreamType::Stderr,
            _ => return None,
        };

        Some(LogEntry {
            stream_type,
            content: content.to_string(),
        })
    })
}

fn load_log_file(file_path: &str) -> io::Result<Vec<LogEntry>> {
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();

    for line in reader.lines() {
        let line = line?;
        if let Some(entry) = parse_log_line(&line) {
            entries.push(entry);
        }
    }

    Ok(entries)
}

pub fn playback(log_file_path: &String) -> io::Result<()> {
    let entries = load_log_file(log_file_path)?;
    let errors_file = File::create(format!("{}.errors.txt", log_file_path))?;

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();

    for entry in entries {
        match entry.stream_type {
            StreamType::Stdout => {
                writeln!(stdout, "{}", entry.content)?;
                stdout.flush()?;
            }
            StreamType::Stderr => {
                writeln!(stderr, "{}", entry.content)?;
                stderr.flush()?;
            }
            StreamType::Stdin => {
                // Wait for matching input
                let mut input = String::new();
                stdin.read_line(&mut input)?;
                input = input.trim_end_matches('\n').to_string();

                let input_value: Value = serde_json::from_str::<Value>(&input)?;
                let entry_value: Value = serde_json::from_str::<Value>(&entry.content)?;
                if input_value != entry_value {
                    writeln!(
                        &errors_file,
                        "expected:\n{}\ngot:\n{}",
                        serde_json::to_string(&entry_value)?,
                        serde_json::to_string(&input_value)?
                    )?;
                    process::exit(1);
                }
            }
        }
    }

    Ok(())
}
