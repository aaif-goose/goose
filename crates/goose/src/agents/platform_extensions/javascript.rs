use boa_engine::{
    builtins::promise::PromiseState,
    job::{Job, JobExecutor},
    js_string,
    module::IdleModuleLoader,
    object::{builtins::JsPromise, ObjectInitializer},
    property::{Attribute, PropertyKey},
    Context, JsArgs, JsError, JsNativeError, JsResult, JsValue, NativeFunction, Script, Source,
};
use futures::{stream::FuturesUnordered, FutureExt, StreamExt};
use serde_json::Value;
use std::{
    cell::RefCell,
    collections::{HashSet, VecDeque},
    future::Future,
    io::{self, Write},
    pin::Pin,
    rc::Rc,
    sync::Arc,
    time::Duration,
};
use tokio_util::sync::CancellationToken;

pub(super) type ToolFuture = Pin<Box<dyn Future<Output = Result<Value, String>> + Send>>;
pub(super) type ToolCallback = Arc<dyn Fn(String, Value) -> ToolFuture + Send + Sync>;

const INSTRUCTION_LIMIT: usize = 2_000_000;
static WORKER_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);
pub(super) const DRAIN_TIMEOUT: Duration = Duration::from_millis(500);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(1);
const OUTPUT_LIMIT: usize = 1_000_000;
const OUTPUT_LIMIT_ERROR: &str =
    "Output limit exceeded; return or log only information needed for the next step";
const ERROR_TRUNCATED: &str = "\n[Error truncated: output limit exceeded]";

pub(super) fn limit_error(mut error: String) -> String {
    if error.len() > OUTPUT_LIMIT {
        let mut end = OUTPUT_LIMIT - ERROR_TRUNCATED.len();
        while !error.is_char_boundary(end) {
            end -= 1;
        }
        error.truncate(end);
        error.push_str(ERROR_TRUNCATED);
    }
    error
}

#[derive(Default)]
struct Output(Vec<u8>);

impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > OUTPUT_LIMIT - self.0.len() {
            return Err(io::Error::other(OUTPUT_LIMIT_ERROR));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct HostState {
    callback: ToolCallback,
    output: Output,
}

#[derive(Default)]
struct Jobs(RefCell<VecDeque<Job>>);

impl JobExecutor for Jobs {
    fn enqueue_job(self: Rc<Self>, job: Job, _: &mut Context) {
        self.0.borrow_mut().push_back(job);
    }

    fn run_jobs(self: Rc<Self>, _: &mut Context) -> JsResult<()> {
        Err(JsNativeError::error()
            .with_message("Jobs must run on the JavaScript worker")
            .into())
    }
}

fn json_value(value: &JsValue, context: &mut Context) -> JsResult<Option<Value>> {
    fn convert(
        value: &JsValue,
        context: &mut Context,
        seen: &mut HashSet<boa_engine::JsObject>,
        remaining: &mut usize,
        depth: usize,
    ) -> JsResult<Option<Value>> {
        if depth > 64 || *remaining == 0 {
            return Err(JsNativeError::range()
                .with_message("JSON value exceeds the depth or item limit")
                .into());
        }
        *remaining -= 1;
        if value.is_callable() {
            return Err(JsNativeError::typ()
                .with_message("Functions are not JSON values")
                .into());
        }
        let Some(object) = value.as_object() else {
            return value.to_json(context);
        };
        if !seen.insert(object.clone()) {
            return Err(JsNativeError::typ()
                .with_message("cyclic object value")
                .into());
        }
        let result = if object.is_array() {
            let length = object.get(js_string!("length"), context)?.to_u32(context)? as usize;
            if length > *remaining {
                return Err(JsNativeError::range()
                    .with_message("JSON array exceeds the item limit")
                    .into());
            }
            let mut array = Vec::with_capacity(length);
            for index in 0..length {
                let item = object.get(index as u32, context)?;
                array.push(
                    convert(&item, context, seen, remaining, depth + 1)?.unwrap_or(Value::Null),
                );
            }
            Value::Array(array)
        } else {
            let mut map = serde_json::Map::new();
            for key in object.own_property_keys(context)? {
                let name = match &key {
                    PropertyKey::String(name) => name.to_std_string_escaped(),
                    PropertyKey::Index(index) => index.get().to_string(),
                    PropertyKey::Symbol(_) => {
                        return Err(JsNativeError::typ()
                            .with_message("Symbols are not JSON values")
                            .into())
                    }
                };
                let item = object.get(key, context)?;
                if let Some(item) = convert(&item, context, seen, remaining, depth + 1)? {
                    map.insert(name, item);
                }
            }
            Value::Object(map)
        };
        seen.remove(&object);
        Ok(Some(result))
    }
    convert(value, context, &mut HashSet::new(), &mut 10_000, 0)
}

async fn call_tool(
    _: &JsValue,
    args: &[JsValue],
    context: &RefCell<&mut Context>,
) -> JsResult<JsValue> {
    let (name, arguments, callback) = {
        let context = &mut context.borrow_mut();
        let name = args.get_or_undefined(0).as_string().ok_or_else(|| {
            JsNativeError::typ().with_message("tools.call requires an exact tool name string; use list_functions to discover names")
        })?.to_std_string_escaped();
        let arguments = json_value(args.get_or_undefined(1), context)
            .map_err(|e| {
                JsNativeError::typ().with_message(format!("{name}: arguments must be JSON: {e}"))
            })?
            .ok_or_else(|| {
                JsNativeError::typ().with_message(format!(
                    "{name}: pass a JSON argument object, using {{}} for no arguments"
                ))
            })?;
        if !arguments.is_object() {
            return Err(JsNativeError::typ()
                .with_message(format!("{name}: arguments must be a JSON object"))
                .into());
        }
        let callback = context
            .get_data::<HostState>()
            .expect("worker host state")
            .callback
            .clone();
        (name, arguments, callback)
    };
    let result = callback(name.clone(), arguments)
        .await
        .map_err(|e| JsNativeError::error().with_message(format!("Tool {name} failed: {e}")))?;
    JsValue::from_json(&result, &mut context.borrow_mut())
}

fn console(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let line = args
        .iter()
        .map(|value| {
            if let Some(string) = value.as_string() {
                Ok(string.to_std_string_escaped())
            } else {
                json_value(value, context)
                    .map(|v| v.map_or_else(|| "undefined".into(), |v| v.to_string()))
            }
        })
        .collect::<JsResult<Vec<_>>>()?
        .join(" ");
    let mut state = context
        .remove_data::<HostState>()
        .expect("worker host state");
    let result = state
        .output
        .write_all(line.as_bytes())
        .and_then(|()| state.output.write_all(b"\n"));
    context.insert_data(*state);
    result.map_err(|_| JsNativeError::range().with_message(OUTPUT_LIMIT_ERROR))?;
    Ok(JsValue::undefined())
}

async fn evaluate(
    code: &str,
    callback: ToolCallback,
    dispatch: CancellationToken,
) -> Result<String, String> {
    let jobs = Rc::new(Jobs::default());
    // Boa exposes its context-wide instruction counter through the fuzz feature.
    // Unlike per-loop limits, it also bounds synchronous promise continuations.
    let mut context = Context::builder()
        .job_executor(jobs.clone())
        .module_loader(Rc::new(IdleModuleLoader))
        .can_block(false)
        .instructions_remaining(INSTRUCTION_LIMIT)
        .build()
        .map_err(|e| e.to_string())?;
    context
        .runtime_limits_mut()
        .set_loop_iteration_limit(100_000);
    context.runtime_limits_mut().set_recursion_limit(128);
    context.insert_data(HostState {
        callback,
        output: Output::default(),
    });
    let tools = ObjectInitializer::new(&mut context)
        .function(
            NativeFunction::from_async_fn(call_tool),
            js_string!("call"),
            2,
        )
        .build();
    context
        .register_global_property(js_string!("tools"), tools, Attribute::READONLY)
        .map_err(|e| e.to_string())?;
    let mut console_object = ObjectInitializer::new(&mut context);
    for method in ["log", "info", "warn", "error", "debug"] {
        console_object.function(NativeFunction::from_fn_ptr(console), js_string!(method), 0);
    }
    let console_object = console_object.build();
    context
        .register_global_property(js_string!("console"), console_object, Attribute::READONLY)
        .map_err(|e| e.to_string())?;

    let source = format!("{code}\n;run();");
    let script = Script::parse(
        Source::from_bytes(&source).with_path(std::path::Path::new("code_mode.js")),
        None,
        &mut context,
    )
    .map_err(|e| format!("JavaScript syntax error: {e}. Define async function run() {{ ... }}."))?;
    let value = script
        .evaluate_async_with_budget(&mut context, 1024)
        .await
        .map_err(|e| e.to_string())?;
    let promise = value
        .as_object()
        .and_then(|obj| JsPromise::from_object(obj).ok())
        .ok_or("Define async function run() { ... } and await tool calls")?;
    let context = RefCell::new(&mut context);
    let mut pending = FuturesUnordered::new();
    let result = async {
        loop {
            let job = jobs.0.borrow_mut().pop_front();
            if let Some(job) = job {
                let result = match job {
                    Job::AsyncJob(job) => {
                        pending.push(job.call(&context));
                        Ok(JsValue::undefined())
                    }
                    Job::PromiseJob(job) => job.call(&mut context.borrow_mut()),
                    Job::GenericJob(job) => job.call(&mut context.borrow_mut()),
                    Job::FinalizationRegistryCleanupJob(_) => Ok(JsValue::undefined()),
                    _ => return Err("Timers are not available in code mode".into()),
                };
                result.map_err(|e| e.to_string())?;
            }
            if let Some(Some(result)) = pending.next().now_or_never() {
                result.map_err(|e| e.to_string())?;
            }
            {
                match promise.state() {
                    PromiseState::Pending => {
                        if jobs.0.borrow().is_empty() && pending.is_empty() {
                            std::future::pending::<()>().await;
                        }
                    }
                    PromiseState::Rejected(value) => {
                        return Err(JsError::from_opaque(value).to_string())
                    }
                    PromiseState::Fulfilled(value) => {
                        let context = &mut context.borrow_mut();
                        let result = json_value(&value, context).map_err(|e| {
                            format!(
                            "Unsupported JavaScript return value; return JSON or undefined: {e}"
                        )
                        })?;
                        let mut state = context
                            .remove_data::<HostState>()
                            .expect("worker host state");
                        let mut output = std::mem::take(&mut state.output);
                        context.insert_data(*state);
                        if let Some(result) = result {
                            output
                                .write_all(b"Result: ")
                                .map_err(|_| OUTPUT_LIMIT_ERROR)?;
                            serde_json::to_writer(&mut output, &result)
                                .map_err(|_| OUTPUT_LIMIT_ERROR)?;
                        }
                        return String::from_utf8(output.0).map_err(|e| e.to_string());
                    }
                }
            }
            if jobs.0.borrow().is_empty() && !pending.is_empty() {
                if let Some(result) = pending.next().await {
                    result.map_err(|e| e.to_string())?;
                }
            } else {
                tokio::task::yield_now().await;
            }
        }
    }
    .await;
    dispatch.cancel();
    let _ = tokio::time::timeout(DRAIN_TIMEOUT, async {
        while pending.next().await.is_some() {}
    })
    .await;
    result
}

pub(super) async fn execute(
    code: String,
    callback: ToolCallback,
    timeout: Duration,
    cancellation: CancellationToken,
    dispatch: CancellationToken,
) -> Result<String, String> {
    execute_with_slots(
        code,
        callback,
        timeout,
        cancellation,
        dispatch,
        &WORKER_SLOTS,
    )
    .await
}

async fn execute_with_slots(
    code: String,
    callback: ToolCallback,
    timeout: Duration,
    cancellation: CancellationToken,
    dispatch: CancellationToken,
    slots: &'static tokio::sync::Semaphore,
) -> Result<String, String> {
    let cancel_on_drop = dispatch.clone().drop_guard();
    let deadline = tokio::time::Instant::now() + timeout;
    let permit = tokio::select! {
        biased;
        _ = cancellation.cancelled() => return Err("Execution cancelled".into()),
        _ = tokio::time::sleep_until(deadline) => {
            return Err(format!("Execution timed out after {} seconds while waiting for a JavaScript worker", timeout.as_secs_f64()));
        }
        permit = slots.acquire() => permit.map_err(|e| format!("JavaScript workers unavailable: {e}"))?,
    };
    let worker_cancellation = cancellation.child_token();
    let stop_worker = worker_cancellation.clone();
    let _stop_on_drop = stop_worker.clone().drop_guard();
    let worker_dispatch = dispatch.clone();
    let (sender, receiver) = tokio::sync::oneshot::channel();
    std::thread::Builder::new().name("goose-javascript".into()).spawn(move || {
        // The worker retains its slot even if the caller stops waiting for it.
        let _permit = permit;
        let result = tokio::runtime::Builder::new_current_thread().enable_all().build()
            .map_err(|e| e.to_string()).and_then(|runtime| runtime.block_on(async {
                let execution = evaluate(&code, callback, worker_dispatch.clone());
                tokio::pin!(execution);
                let result = tokio::select! {
                    biased;
                    _ = worker_cancellation.cancelled() => Err("Execution cancelled".into()),
                    _ = tokio::time::sleep_until(deadline) => Err(format!("Execution timed out after {} seconds", timeout.as_secs_f64())),
                    result = &mut execution => {
                        worker_dispatch.cancel();
                        return result;
                    },
                };
                worker_dispatch.cancel();
                // Let dispatched tools observe cancellation and perform cleanup before
                // dropping their futures (which aborts any remaining dispatch tasks).
                let _ = tokio::time::timeout(DRAIN_TIMEOUT, &mut execution).await;
                result
            }));
        let _ = sender.send(result);
    }).map_err(|e| format!("Could not start JavaScript worker: {e}"))?;
    tokio::pin!(receiver);
    let result = tokio::select! {
        biased;
        _ = cancellation.cancelled() => {
            dispatch.cancel();
            stop_worker.cancel();
            // Synchronous Boa work may not yield; do not wait indefinitely for it.
            let _ = tokio::time::timeout(SHUTDOWN_TIMEOUT, receiver).await;
            Err("Execution cancelled".into())
        }
        _ = tokio::time::sleep_until(deadline) => {
            dispatch.cancel();
            stop_worker.cancel();
            let _ = tokio::time::timeout(SHUTDOWN_TIMEOUT, receiver).await;
            Err(format!("Execution timed out after {} seconds", timeout.as_secs_f64()))
        }
        result = &mut receiver => result.map_err(|e| format!("JavaScript worker failed: {e}"))?,
    };
    drop(cancel_on_drop);
    result.map_err(limit_error)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    async fn execute(
        code: String,
        callback: ToolCallback,
        timeout: Duration,
        cancellation: CancellationToken,
        dispatch: CancellationToken,
    ) -> Result<String, String> {
        // Keep timeout-focused tests independent of pool saturation, tested separately below.
        static SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(32);
        execute_with_slots(code, callback, timeout, cancellation, dispatch, &SLOTS).await
    }

    #[tokio::test]
    async fn worker_capacity_bounds_concurrency_and_waiting_is_cancellable() {
        static SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let started = Arc::new(tokio::sync::Notify::new());
        let release = CancellationToken::new();
        let callback: ToolCallback = Arc::new({
            let active = active.clone();
            let peak = peak.clone();
            let started = started.clone();
            let release = release.clone();
            move |_, _| {
                let active = active.clone();
                let peak = peak.clone();
                let started = started.clone();
                let release = release.clone();
                Box::pin(async move {
                    struct Active(Arc<AtomicUsize>);
                    impl Drop for Active {
                        fn drop(&mut self) {
                            self.0.fetch_sub(1, Ordering::SeqCst);
                        }
                    }
                    let count = active.fetch_add(1, Ordering::SeqCst) + 1;
                    let _active = Active(active);
                    peak.fetch_max(count, Ordering::SeqCst);
                    started.notify_one();
                    release.cancelled().await;
                    Ok(json!(true))
                })
            }
        });
        let spawn = || {
            tokio::spawn(execute_with_slots(
                "async function run() { return await tools.call('wait', {}); }".into(),
                callback.clone(),
                Duration::from_secs(10),
                CancellationToken::new(),
                CancellationToken::new(),
                &SLOTS,
            ))
        };
        let first = spawn();
        let second = spawn();
        tokio::time::timeout(Duration::from_secs(2), async {
            while active.load(Ordering::SeqCst) < 2 {
                started.notified().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(SLOTS.available_permits(), 0);
        let dispatch = CancellationToken::new();
        let error = execute_with_slots(
            "async function run() { return 42; }".into(),
            echo(),
            Duration::from_millis(50),
            CancellationToken::new(),
            dispatch.clone(),
            &SLOTS,
        )
        .await
        .unwrap_err();
        assert!(error.contains("waiting for a JavaScript worker"), "{error}");
        assert!(dispatch.is_cancelled());
        let token = CancellationToken::new();
        let dispatch = CancellationToken::new();
        let queued = execute_with_slots(
            "async function run() { return 42; }".into(),
            echo(),
            Duration::from_secs(10),
            token.clone(),
            dispatch.clone(),
            &SLOTS,
        );
        tokio::pin!(queued);
        assert!(queued.as_mut().now_or_never().is_none());
        token.cancel();
        assert_eq!(queued.await.unwrap_err(), "Execution cancelled");
        assert!(dispatch.is_cancelled());
        release.cancel();
        assert_eq!(first.await.unwrap().unwrap(), "Result: true");
        assert_eq!(second.await.unwrap().unwrap(), "Result: true");
        assert_eq!(peak.load(Ordering::SeqCst), 2);
        assert_eq!(
            execute_with_slots(
                "async function run() { return 42; }".into(),
                echo(),
                Duration::from_secs(2),
                CancellationToken::new(),
                CancellationToken::new(),
                &SLOTS,
            )
            .await
            .unwrap(),
            "Result: 42"
        );
    }

    #[tokio::test]
    async fn abandoned_worker_retains_capacity_until_it_finishes() {
        static SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
        let started = Arc::new(tokio::sync::Notify::new());
        let (release, blocked) = std::sync::mpsc::channel();
        let blocked = std::sync::Mutex::new(blocked);
        let callback: ToolCallback = Arc::new({
            let started = started.clone();
            move |_, _| {
                started.notify_one();
                let _ = blocked.lock().unwrap().recv_timeout(Duration::from_secs(5));
                Box::pin(async { Ok(Value::Null) })
            }
        });
        let token = CancellationToken::new();
        let execution = tokio::spawn(execute_with_slots(
            "async function run() { await tools.call('block', {}); }".into(),
            callback,
            Duration::from_secs(10),
            token.clone(),
            CancellationToken::new(),
            &SLOTS,
        ));
        tokio::time::timeout(Duration::from_secs(2), started.notified())
            .await
            .unwrap();
        token.cancel();
        let result = tokio::time::timeout(Duration::from_secs(2), execution).await;
        let held = SLOTS.available_permits() == 0;
        let queued = execute_with_slots(
            "async function run() { return 7; }".into(),
            echo(),
            Duration::from_millis(50),
            CancellationToken::new(),
            CancellationToken::new(),
            &SLOTS,
        )
        .await;
        let _ = release.send(());
        assert_eq!(result.unwrap().unwrap().unwrap_err(), "Execution cancelled");
        assert!(held);
        assert!(queued
            .unwrap_err()
            .contains("waiting for a JavaScript worker"));
        assert_eq!(
            execute_with_slots(
                "async function run() { return 9; }".into(),
                echo(),
                Duration::from_secs(2),
                CancellationToken::new(),
                CancellationToken::new(),
                &SLOTS,
            )
            .await
            .unwrap(),
            "Result: 9"
        );
    }

    fn echo() -> ToolCallback {
        Arc::new(|name, args| {
            Box::pin(async move {
                match name.as_str() {
                    "echo" => Ok(args),
                    "fail" => Err("permission denied".into()),
                    _ => Err("Unknown tool; use list_functions and get_function_details".into()),
                }
            })
        })
    }

    async fn run(code: &str) -> Result<String, String> {
        execute(
            code.into(),
            echo(),
            Duration::from_millis(300),
            CancellationToken::new(),
            CancellationToken::new(),
        )
        .await
    }

    #[tokio::test]
    async fn sequential_parallel_and_caught_errors() {
        let result = run(r#"async function run() {
            const a = await tools.call("echo", {value: 1});
            const both = await Promise.all([tools.call("echo", {value: a.value + 1}), tools.call("echo", {value: 3})]);
            try { await tools.call("fail", {}); } catch (e) { console.log(e.message); }
            console.info("done", both);
            return both;
        }"#).await.unwrap();
        assert!(
            result.contains("Tool fail failed: permission denied"),
            "{result}"
        );
        assert!(
            result.contains(r#"Result: [{"value":2},{"value":3}]"#),
            "{result}"
        );
    }

    #[tokio::test]
    async fn calls_are_actually_parallel() {
        let started = Arc::new(AtomicUsize::new(0));
        let callback: ToolCallback = Arc::new(move |_, _| {
            let started = started.clone();
            Box::pin(async move {
                started.fetch_add(1, Ordering::SeqCst);
                while started.load(Ordering::SeqCst) < 2 {
                    tokio::task::yield_now().await;
                }
                Ok(json!(true))
            })
        });
        let result = execute("async function run() { return await Promise.all([tools.call('one', {}), tools.call('two', {})]); }".into(), callback, Duration::from_secs(2), CancellationToken::new(), CancellationToken::new()).await.unwrap();
        assert!(result.contains("[true,true]"));
    }

    #[tokio::test]
    async fn mistakes_are_actionable() {
        for (code, expected) in [
            ("async function run() { const = 1; }", "syntax error"),
            (
                "async function run() { throw new Error('failure'); }",
                "failure",
            ),
            ("async function run() { return 1n; }", "bigint"),
            (
                "async function run() { const x = {}; x.x = x; return x; }",
                "cyclic",
            ),
            (
                "async function run() { await tools.call('echo', 1); }",
                "JSON object",
            ),
            (
                "async function run() { await tools.call('missing', {}); }",
                "list_functions",
            ),
        ] {
            let error = run(code).await.unwrap_err();
            assert!(error.contains(expected), "{error}");
        }
    }

    #[tokio::test]
    async fn runaway_scripts_stop_and_next_context_is_fresh() {
        for code in [
            "async function run() { while (true) {} }",
            "async function run() { function recurse() { recurse(); } recurse(); }",
            "async function run() { await new Promise(() => {}); }",
            "async function run() { await Promise.resolve(); while (true) {} }",
            "async function run() { await Promise.resolve().then(() => { while (true) {} }); }",
            "async function run() { while (true) { await Promise.resolve(); } }",
            "async function run() { while (true) { for (let i=0; i<90000; i++) {} } }",
        ] {
            assert!(run(code).await.is_err(), "{code}");
            assert_eq!(
                run("async function run() { return 42; }").await.unwrap(),
                "Result: 42"
            );
        }
        run("async function run() { globalThis.secret = 42; }")
            .await
            .unwrap();
        assert_eq!(
            run("async function run() { return typeof secret; }")
                .await
                .unwrap(),
            "Result: \"undefined\""
        );
    }

    #[tokio::test]
    async fn cancellation_releases_callbacks_and_other_sessions_run() {
        let token = CancellationToken::new();
        let dispatch = token.child_token();
        let observed = Arc::new(AtomicBool::new(false));
        let callback: ToolCallback = Arc::new({
            let dispatch = dispatch.clone();
            let observed = observed.clone();
            move |_, _| {
                let dispatch = dispatch.clone();
                let observed = observed.clone();
                Box::pin(async move {
                    dispatch.cancelled().await;
                    observed.store(true, Ordering::SeqCst);
                    Ok(Value::Null)
                })
            }
        });
        let execution = execute(
            "async function run() { await tools.call('wait', {}); await new Promise(() => {}); }"
                .into(),
            callback,
            Duration::from_secs(2),
            token.clone(),
            dispatch,
        );
        let cancel = async {
            tokio::time::sleep(Duration::from_millis(30)).await;
            token.cancel();
        };
        let (result, _, normal) =
            tokio::join!(execution, cancel, run("async function run() { return 7; }"));
        assert_eq!(result.unwrap_err(), "Execution cancelled");
        assert!(observed.load(Ordering::SeqCst));
        assert_eq!(normal.unwrap(), "Result: 7");
        assert_eq!(
            run("async function run() { return 8; }").await.unwrap(),
            "Result: 8"
        );
    }

    #[tokio::test]
    async fn host_capabilities_are_absent() {
        let output = run("async function run() { return [typeof fetch, typeof process, typeof Deno, typeof require, typeof setTimeout]; }").await.unwrap();
        assert_eq!(
            output,
            "Result: [\"undefined\",\"undefined\",\"undefined\",\"undefined\",\"undefined\"]"
        );
        assert!(
            run("async function run() { return await import('file:///etc/passwd'); }")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn output_limit_includes_serialized_results_and_console_output() {
        for body in [
            "return 'xxxxxxxxxxxxxxxx'.repeat(62500);",
            "console.log('xxxxxxxxxxxxxxxx'.repeat(62500));",
            "console.log('xxxxxxxxxxxxxxxx'.repeat(37500)); return 'yyyyyyyyyyyyyyyy'.repeat(37500);",
            "return ['xxxxxxxxxxxxxxxx'.repeat(37500), 'yyyyyyyyyyyyyyyy'.repeat(37500)];",
            "return '\\u0000\\u0000\\u0000\\u0000'.repeat(50000);",
            "return '\\u20ac\\u20ac\\u20ac\\u20ac\\u20ac'.repeat(70000);",
        ] {
            let error = execute(
                format!("async function run() {{ {body} }}"),
                echo(),
                Duration::from_secs(5),
                CancellationToken::new(),
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
            assert!(error.contains(OUTPUT_LIMIT_ERROR), "{body}: {error}");
        }
        let length = OUTPUT_LIMIT - "ok\nResult: \"\"".len();
        let result = execute(
            format!(
                "async function run() {{ console.log('ok'); return 'xxxxxxxxxxxxxxxx'.repeat({}) + 'x'.repeat({}); }}",
                length / 16,
                length % 16,
            ),
            echo(),
            Duration::from_secs(5),
            CancellationToken::new(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(result.len(), OUTPUT_LIMIT);
        assert!(result.starts_with("ok\nResult: \""));
        assert_eq!(
            run("async function run() { return 42; }").await.unwrap(),
            "Result: 42"
        );
    }

    #[tokio::test]
    async fn rejected_values_and_callback_errors_are_bounded() {
        for body in [
            "throw new Error('xxxxxxxxxxxxxxxx'.repeat(62500));",
            "throw 'xxxxxxxxxxxxxxxx'.repeat(62500);",
            "await Promise.reject(new Error('\u{20ac}\u{20ac}\u{20ac}\u{20ac}\u{20ac}'.repeat(70000)));",
        ] {
            let error = execute(
                format!("async function run() {{ {body} }}"),
                echo(),
                Duration::from_secs(5),
                CancellationToken::new(),
                CancellationToken::new(),
            ).await.unwrap_err();
            assert!(error.len() <= OUTPUT_LIMIT);
            assert!(error.ends_with(ERROR_TRUNCATED));
        }
        let callback: ToolCallback =
            Arc::new(|_, _| Box::pin(async { Err("server error ".repeat(100_000)) }));
        let error = execute(
            "async function run() { await tools.call('example__fail', {}); }".into(),
            callback,
            Duration::from_secs(5),
            CancellationToken::new(),
            CancellationToken::new(),
        )
        .await
        .unwrap_err();
        assert!(error.len() <= OUTPUT_LIMIT);
        assert!(error.contains("example__fail"));
        assert!(error.ends_with(ERROR_TRUNCATED));
        assert_eq!(
            run("async function run() { return 7; }").await.unwrap(),
            "Result: 7"
        );
    }

    #[tokio::test]
    async fn synchronous_worker_work_does_not_block_cancellation_or_timeout() {
        for cancel in [true, false] {
            let started = Arc::new(tokio::sync::Notify::new());
            let (release, blocked) = std::sync::mpsc::channel();
            let blocked = Arc::new(std::sync::Mutex::new(blocked));
            let callback: ToolCallback = Arc::new({
                let started = started.clone();
                move |_, _| {
                    started.notify_one();
                    // Simulate synchronous native work that cannot poll cancellation.
                    let _ = blocked.lock().unwrap().recv_timeout(Duration::from_secs(5));
                    Box::pin(async { Ok(Value::Null) })
                }
            });
            let token = CancellationToken::new();
            let dispatch = CancellationToken::new();
            let execution = execute(
                "async function run() { return await tools.call('block', {}); }".into(),
                callback,
                if cancel {
                    Duration::from_secs(10)
                } else {
                    Duration::from_millis(100)
                },
                token.clone(),
                dispatch.clone(),
            );
            let control = async {
                started.notified().await;
                if cancel {
                    token.cancel();
                }
            };
            let (result, _) = tokio::join!(
                tokio::time::timeout(Duration::from_secs(2), execution),
                control,
            );
            let _ = release.send(());
            let error = result
                .expect("cancellation must not wait for synchronous work")
                .unwrap_err();
            assert!(
                error.contains(if cancel { "cancelled" } else { "timed out" }),
                "{error}"
            );
            assert!(dispatch.is_cancelled());
            assert_eq!(
                run("async function run() { return 7; }").await.unwrap(),
                "Result: 7"
            );
        }
    }
    #[tokio::test]
    async fn cancellation_in_promise_continuations_drains_in_flight_calls() {
        for continuation in [
            "await Promise.resolve(); while (true) { for (let i=0; i<90000; i++) {} }",
            "await Promise.resolve().then(() => { while (true) { for (let i=0; i<90000; i++) {} } });",
            "while (true) { await Promise.resolve(); }",
        ] {
            let token = CancellationToken::new();
            let dispatch = token.child_token();
            let started = Arc::new(tokio::sync::Notify::new());
            let observed = Arc::new(AtomicBool::new(false));
            let callback: ToolCallback = Arc::new({
                let dispatch = dispatch.clone();
                let started = started.clone();
                let observed = observed.clone();
                move |_, _| {
                    let dispatch = dispatch.clone();
                    let started = started.clone();
                    let observed = observed.clone();
                    Box::pin(async move {
                        started.notify_one();
                        dispatch.cancelled().await;
                        observed.store(true, Ordering::SeqCst);
                        Ok(Value::Null)
                    })
                }
            });
            let code = format!("async function run() {{ const waiting = tools.call('wait', {{}}); {continuation} }}");
            let execution = execute(code, callback, Duration::from_secs(2), token.clone(), dispatch);
            let cancel = async { started.notified().await; token.cancel(); };
            let (result, _) = tokio::join!(execution, cancel);
            assert_eq!(result.unwrap_err(), "Execution cancelled");
            assert!(observed.load(Ordering::SeqCst));
            assert_eq!(run("async function run() { return 9; }").await.unwrap(), "Result: 9");
        }
    }

    #[tokio::test]
    async fn timeout_drops_uncooperative_callbacks() {
        struct Released(Arc<AtomicBool>);
        impl Drop for Released {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let released = Arc::new(AtomicBool::new(false));
        let callback: ToolCallback = Arc::new({
            let released = released.clone();
            move |_, _| {
                let guard = Released(released.clone());
                Box::pin(async move {
                    let _guard = guard;
                    std::future::pending().await
                })
            }
        });
        let result = execute(
            "async function run() { await tools.call('wait', {}); }".into(),
            callback,
            Duration::from_millis(30),
            CancellationToken::new(),
            CancellationToken::new(),
        )
        .await;
        assert!(result.unwrap_err().contains("timed out"));
        assert!(released.load(Ordering::SeqCst));
        assert_eq!(
            run("async function run() { return 10; }").await.unwrap(),
            "Result: 10"
        );
    }
    #[tokio::test]
    async fn errors_include_source_locations_and_stacks() {
        let syntax = run("async function run() {\n const = 1;\n }")
            .await
            .unwrap_err();
        assert!(
            syntax.contains("line 2") && syntax.contains("col"),
            "{syntax}"
        );
        let runtime = run(
            "async function run() { function broken() { throw new Error('boom'); } broken(); }",
        )
        .await
        .unwrap_err();
        assert!(
            runtime.contains("broken") && runtime.contains("code_mode.js"),
            "{runtime}"
        );
        assert!(run("async function run() { return () => 1; }")
            .await
            .unwrap_err()
            .contains("Functions are not JSON"));
    }
    #[tokio::test]
    async fn failed_execution_cancels_and_drains_sibling_callbacks() {
        let dispatch = CancellationToken::new();
        let observed = Arc::new(AtomicBool::new(false));
        let callback: ToolCallback = Arc::new({
            let dispatch = dispatch.clone();
            let observed = observed.clone();
            move |name, _| {
                let dispatch = dispatch.clone();
                let observed = observed.clone();
                Box::pin(async move {
                    if name == "fail" {
                        return Err("denied".into());
                    }
                    dispatch.cancelled().await;
                    observed.store(true, Ordering::SeqCst);
                    Ok(Value::Null)
                })
            }
        });
        let result = execute("async function run() { return await Promise.all([tools.call('fail', {}), tools.call('wait', {})]); }".into(), callback, Duration::from_secs(2), CancellationToken::new(), dispatch).await;
        assert!(result.unwrap_err().contains("Tool fail failed: denied"));
        assert!(observed.load(Ordering::SeqCst));
        assert_eq!(
            run("async function run() { return 11; }").await.unwrap(),
            "Result: 11"
        );
    }
}
