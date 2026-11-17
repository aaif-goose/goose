use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use goose::config::Config;
use std::io::{IsTerminal, Write};
use std::sync::Mutex;

const MAX_MSG_BYTES: usize = 2048;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BlockedKind {
    Permission,
    Question,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProgramStatus {
    Idle,
    Working,
    Done,
    Error,
    Blocked(BlockedKind),
    Clear,
}

struct Reporter {
    enabled: bool,
    last: Option<ProgramStatus>,
}

static REPORTER: Mutex<Reporter> = Mutex::new(Reporter {
    enabled: false,
    last: None,
});

pub fn enable(text_output: bool) {
    REPORTER.lock().unwrap().enabled = text_output
        && std::io::stdout().is_terminal()
        && Config::global()
            .get_param::<bool>("GOOSE_CLI_PROGRAM_STATUS")
            .unwrap_or(true);
}

pub fn disable() {
    *REPORTER.lock().unwrap() = Reporter {
        enabled: false,
        last: None,
    };
}

pub fn report(status: ProgramStatus, msg: Option<&str>) {
    let mut reporter = REPORTER.lock().unwrap();
    if !reporter.enabled {
        return;
    }
    reporter.last = Some(status);
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(encode(status, msg).as_bytes());
    let _ = stdout.flush();
}

pub fn report_idle() {
    let last = REPORTER.lock().unwrap().last;
    if !matches!(last, Some(ProgramStatus::Done | ProgramStatus::Error)) {
        report(ProgramStatus::Idle, None);
    }
}

fn encode(status: ProgramStatus, msg: Option<&str>) -> String {
    let state = match status {
        ProgramStatus::Idle => "idle",
        ProgramStatus::Working => "working",
        ProgramStatus::Done => "done",
        ProgramStatus::Error => "error",
        ProgramStatus::Blocked(_) => "blocked",
        ProgramStatus::Clear => "clear",
    };
    let mut body = format!("state={state}");
    match status {
        ProgramStatus::Blocked(BlockedKind::Permission) => body.push_str(":kind=permission"),
        ProgramStatus::Blocked(BlockedKind::Question) => body.push_str(":kind=question"),
        _ => {}
    }
    body.push_str(":app=goose");
    if let Some(msg) = msg {
        let line: String = msg
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        let line = line.trim();
        let line = line
            .get(..line.floor_char_boundary(MAX_MSG_BYTES))
            .unwrap_or_default();
        if !line.is_empty() {
            body.push_str(":msg=");
            body.push_str(&STANDARD.encode(line));
        }
    }
    format!("\x1b]7501;{body}\x1b\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_sanitizes_and_caps_msg() {
        assert_eq!(
            encode(
                ProgramStatus::Blocked(BlockedKind::Permission),
                Some("run\n\x1b]0;x\x07 shell")
            ),
            format!(
                "\x1b]7501;state=blocked:kind=permission:app=goose:msg={}\x1b\\",
                STANDARD.encode("run  ]0;x  shell")
            )
        );

        let long = "é".repeat(MAX_MSG_BYTES);
        let encoded = encode(ProgramStatus::Done, Some(&long));
        let msg = encoded
            .strip_prefix("\x1b]7501;state=done:app=goose:msg=")
            .and_then(|s| s.strip_suffix("\x1b\\"))
            .unwrap();
        assert_eq!(
            STANDARD.decode(msg).unwrap(),
            "é".repeat(MAX_MSG_BYTES / 2).as_bytes()
        );
    }
}
