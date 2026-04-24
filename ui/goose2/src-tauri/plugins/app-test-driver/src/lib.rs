use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, Runtime};
#[cfg(target_os = "macos")]
use tauri::WebviewWindow;

#[derive(Deserialize, Debug)]
struct TestCommand {
    action: String,
    selector: Option<String>,
    value: Option<String>,
    timeout: Option<u64>,
}

#[derive(Serialize)]
struct TestResult {
    success: bool,
    data: Option<String>,
    error: Option<String>,
}

#[tauri::command]
fn driver_result(state: tauri::State<'_, DriverState>, value: String) {
    let mut result = state.pending_result.lock().unwrap();
    *result = Some(value);
    state.signal.notify_one();
}

struct DriverState {
    command_lock: Mutex<()>,
    pending_result: Mutex<Option<String>>,
    signal: std::sync::Condvar,
}

impl DriverState {
    fn new() -> Self {
        Self {
            command_lock: Mutex::new(()),
            pending_result: Mutex::new(None),
            signal: std::sync::Condvar::new(),
        }
    }

    fn wait_for_result(&self, timeout_ms: u64) -> Option<String> {
        let timeout = std::time::Duration::from_millis(timeout_ms + 1000);
        let guard = self.pending_result.lock().unwrap();
        let (mut guard, _) = self
            .signal
            .wait_timeout_while(guard, timeout, |result| result.is_none())
            .unwrap();
        guard.take()
    }

    fn reset(&self) {
        *self.pending_result.lock().unwrap() = None;
    }
}

fn escape_js_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn with_wait_for(selector: &str, action_js: &str, timeout_ms: u64) -> String {
    let escaped = escape_js_string(selector);
    format!(
        r#"(async function() {{
            const sel = "{escaped}";
            const start = Date.now();
            while (Date.now() - start < {timeout_ms}) {{
                const el = document.querySelector(sel);
                if (el) {{
                    {action_js}
                }}
                await new Promise(r => setTimeout(r, 100));
            }}
            return "ERROR: timeout waiting for element: " + sel;
        }})()"#
    )
}

fn build_js(cmd: &TestCommand) -> String {
    let timeout_ms = cmd.timeout.unwrap_or(5000);
    let inner_js = match cmd.action.as_str() {
        "snapshot" => r#"
            (function() {
                const result = [];
                let eIdx = 0;
                let tIdx = 0;

                function isVisible(el) {
                    const style = window.getComputedStyle(el);
                    return style.display !== 'none'
                        && style.visibility !== 'hidden'
                        && style.opacity !== '0';
                }

                function isInteractive(tag) {
                    return ['INPUT','BUTTON','SELECT','TEXTAREA','A'].includes(tag);
                }

                function walk(node, depth) {
                    if (node.nodeType === Node.ELEMENT_NODE) {
                        const tag = node.tagName;
                        if (['SCRIPT','STYLE','META','LINK','NOSCRIPT'].includes(tag)) return;
                        if (!isVisible(node)) return;

                        const indent = '  '.repeat(depth);
                        const tagLower = tag.toLowerCase();

                        if (isInteractive(tag)) {
                            eIdx++;
                            node.setAttribute('data-tid', 'e' + eIdx);
                            let info = '[e' + eIdx + '] ' + tagLower;
                            if (node.type) info += ' type="' + node.type + '"';
                            if (node.placeholder) info += ' placeholder="' + node.placeholder + '"';
                            if (node.value) info += ' value="' + node.value + '"';
                            if (node.href) info += ' href="' + node.href + '"';
                            const text = node.innerText?.trim();
                            if (text && text.length < 100) info += ' "' + text + '"';
                            result.push(indent + info);
                        } else {
                            const directText = Array.from(node.childNodes)
                                .filter(n => n.nodeType === Node.TEXT_NODE)
                                .map(n => n.textContent.trim())
                                .join(' ')
                                .trim();
                            if (directText && directText.length > 0 && directText.length < 200) {
                                tIdx++;
                                result.push(indent + '[t' + tIdx + '] ' + tagLower + ' "' + directText + '"');
                            }
                        }

                        for (const child of node.children) {
                            walk(child, depth + 1);
                        }
                    }
                }

                walk(document.body, 0);
                return result.join('\n');
            })()
        "#
        .to_string(),
        "click" => {
            let sel = cmd.selector.as_deref().unwrap_or("body");
            with_wait_for(sel, r#"el.click(); return "clicked";"#, timeout_ms)
        }
        "fill" => {
            let sel = cmd.selector.as_deref().unwrap_or("input");
            let val = escape_js_string(cmd.value.as_deref().unwrap_or(""));
            with_wait_for(
                sel,
                &format!(
                    r#"const proto = el instanceof HTMLTextAreaElement
                    ? HTMLTextAreaElement.prototype
                    : HTMLInputElement.prototype;
                const setter = Object.getOwnPropertyDescriptor(proto, 'value').set;
                setter.call(el, "{val}");
                el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                return "filled";"#
                ),
                timeout_ms,
            )
        }
        "keypress" => {
            let sel = cmd.selector.as_deref().unwrap_or("body");
            let key = cmd.value.as_deref().unwrap_or("Enter");
            let escaped_key = escape_js_string(key);
            with_wait_for(
                sel,
                &format!(
                    r#"const opts = {{ key: "{escaped_key}", code: "{escaped_key}", keyCode: "{escaped_key}" === "Enter" ? 13 : 0, bubbles: true, cancelable: true }};
                el.dispatchEvent(new KeyboardEvent('keydown', opts));
                el.dispatchEvent(new KeyboardEvent('keypress', opts));
                el.dispatchEvent(new KeyboardEvent('keyup', opts));
                return "keypressed";"#
                ),
                timeout_ms,
            )
        }
        "getText" => {
            let sel = cmd.selector.as_deref().unwrap_or("body");
            with_wait_for(sel, "return el.innerText;", timeout_ms)
        }
        "waitForText" => {
            let sel = cmd.selector.as_deref().unwrap_or("body");
            let text = escape_js_string(cmd.value.as_deref().unwrap_or(""));
            let escaped_sel = escape_js_string(sel);
            format!(
                r#"(async function() {{
                    const sel = "{escaped_sel}";
                    const text = "{text}";
                    const start = Date.now();
                    while (Date.now() - start < {timeout_ms}) {{
                        const el = document.querySelector(sel);
                        if (el && el.innerText.includes(text)) {{
                            return el.innerText;
                        }}
                        await new Promise(r => setTimeout(r, 100));
                    }}
                    return "ERROR: timeout waiting for text: " + text;
                }})()"#
            )
        }
        "count" => {
            let sel = cmd.selector.as_deref().unwrap_or("*");
            let escaped_sel = escape_js_string(sel);
            format!("String(document.querySelectorAll(\"{escaped_sel}\").length)")
        }
        "scroll" => {
            let direction = cmd.value.as_deref().unwrap_or("down");
            match direction {
                "up" => "window.scrollBy(0, -window.innerHeight); 'scrolled up'".to_string(),
                "top" => "window.scrollTo(0, 0); 'scrolled to top'".to_string(),
                "bottom" => {
                    "window.scrollTo(0, document.body.scrollHeight); 'scrolled to bottom'"
                        .to_string()
                }
                _ => "window.scrollBy(0, window.innerHeight); 'scrolled down'".to_string(),
            }
        }
        _ => format!("'unknown action: {}'", cmd.action),
    };

    format!(
        r#"
        (async function() {{
            try {{
                const result = await Promise.resolve({inner_js});
                await window.__TAURI_INTERNALS__.invoke('plugin:app-test-driver|driver_result', {{ value: String(result) }});
            }} catch(e) {{
                await window.__TAURI_INTERNALS__.invoke('plugin:app-test-driver|driver_result', {{ value: 'ERROR: ' + e.message }});
            }}
        }})();
        "#
    )
}

#[cfg(target_os = "macos")]
fn get_ns_window_number<R: Runtime>(window: &WebviewWindow<R>) -> Option<u32> {
    let ns_window_ptr = window.ns_window().ok()?;
    let ns_window = unsafe { &*(ns_window_ptr as *const objc2_app_kit::NSWindow) };
    Some(ns_window.windowNumber() as u32)
}

#[cfg(target_os = "macos")]
fn take_screenshot<R: Runtime>(window: &WebviewWindow<R>, path: &str) -> TestResult {
    if let Some(parent) = std::path::Path::new(path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let window_id = match get_ns_window_number(window) {
        Some(id) => id,
        None => {
            return TestResult {
                success: false,
                data: None,
                error: Some("Failed to get window ID".into()),
            };
        }
    };

    match std::process::Command::new("screencapture")
        .args(["-x", "-l", &window_id.to_string(), path])
        .output()
    {
        Ok(output) if output.status.success() => TestResult {
            success: true,
            data: Some(format!("Screenshot saved to {}", path)),
            error: None,
        },
        Ok(output) => TestResult {
            success: false,
            data: None,
            error: Some(format!(
                "screencapture failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )),
        },
        Err(e) => TestResult {
            success: false,
            data: None,
            error: Some(format!("Failed to run screencapture: {}", e)),
        },
    }
}

fn start_server<R: Runtime>(app_handle: AppHandle<R>) {
    std::thread::spawn(move || {
        let port =
            std::env::var("APP_TEST_DRIVER_PORT").unwrap_or_else(|_| "9999".to_string());
        let addr = format!("127.0.0.1:{port}");
        let listener = match TcpListener::bind(&addr) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[app-test-driver] Failed to bind {addr}: {e}");
                return;
            }
        };
        log::info!("[app-test-driver] Listening on {addr}");

        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let app = app_handle.clone();

            std::thread::spawn(move || {
                let reader = BufReader::new(stream.try_clone().unwrap());

                for line in reader.lines() {
                    let Ok(line) = line else { break };
                    if line.trim().is_empty() {
                        continue;
                    }

                    let cmd: TestCommand = match serde_json::from_str(&line) {
                        Ok(c) => c,
                        Err(e) => {
                            let resp = TestResult {
                                success: false,
                                data: None,
                                error: Some(format!("Invalid JSON: {e}")),
                            };
                            let _ = writeln!(stream, "{}", serde_json::to_string(&resp).unwrap());
                            continue;
                        }
                    };

                    log::info!("[app-test-driver] Received: {cmd:?}");

                    let window = match app.get_webview_window("main") {
                        Some(w) => w,
                        None => {
                            let resp = TestResult {
                                success: false,
                                data: None,
                                error: Some("Main window not found".into()),
                            };
                            let _ = writeln!(stream, "{}", serde_json::to_string(&resp).unwrap());
                            continue;
                        }
                    };

                    let state = app.state::<DriverState>();
                    let _command_guard = state.command_lock.lock().unwrap();

                    #[cfg(target_os = "macos")]
                    if cmd.action == "screenshot" {
                        let path = cmd.value.as_deref().unwrap_or("screenshot.png");
                        let resp = take_screenshot(&window, path);
                        let _ = writeln!(stream, "{}", serde_json::to_string(&resp).unwrap());
                        continue;
                    }

                    state.reset();

                    let js = build_js(&cmd);
                    if let Err(e) = window.eval(&js) {
                        let resp = TestResult {
                            success: false,
                            data: None,
                            error: Some(format!("eval failed: {e}")),
                        };
                        let _ = writeln!(stream, "{}", serde_json::to_string(&resp).unwrap());
                        continue;
                    }

                    let timeout_ms = cmd.timeout.unwrap_or(5000);
                    let result = state.wait_for_result(timeout_ms);
                    let resp = match result {
                        Some(data) if data.starts_with("ERROR:") => TestResult {
                            success: false,
                            data: None,
                            error: Some(data),
                        },
                        Some(data) => TestResult {
                            success: true,
                            data: Some(data),
                            error: None,
                        },
                        None => TestResult {
                            success: false,
                            data: None,
                            error: Some("Timeout waiting for result".into()),
                        },
                    };

                    let _ = writeln!(stream, "{}", serde_json::to_string(&resp).unwrap());
                }
            });
        }
    });
}

pub fn init<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("app-test-driver")
        .invoke_handler(tauri::generate_handler![driver_result])
        .setup(|app, _api| {
            app.manage(DriverState::new());
            start_server(app.clone());
            Ok(())
        })
        .build()
}
