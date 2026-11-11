use rmcp::{
    model::{CancelledNotificationParam, ServerResult},
    service::RequestHandle,
    RoleClient, ServiceError,
};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Await a request while retaining cancellation ownership if the caller future is dropped.
pub async fn await_response(
    handle: RequestHandle<RoleClient>,
    timeout: Duration,
    cancel: &CancellationToken,
) -> Result<ServerResult, ServiceError> {
    let peer = handle.peer.clone();
    let request_id = handle.id.clone();
    let child = cancel.child_token();
    let guard = child.clone().drop_guard();
    let done = CancellationToken::new();
    let finished = done.clone();
    let watcher = async move {
        tokio::select! {
            biased;
            _ = finished.cancelled() => {},
            _ = child.cancelled() => { let _ = peer.notify_cancelled(CancelledNotificationParam::new(Some(request_id), Some("request interrupted".into()))).await; }
        }
    };
    #[cfg(not(target_arch = "wasm32"))]
    tokio::spawn(watcher);
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(watcher);
    let response = handle.await_response();
    tokio::pin!(response);
    tokio::select! {
        biased;
        result = &mut response => { done.cancel(); guard.disarm(); result },
        _ = cancel.cancelled() => Err(ServiceError::Cancelled { reason: None }),
        _ = sleep(timeout) => Err(ServiceError::Timeout { timeout }),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) async fn sleep(duration: Duration) {
    tokio::time::sleep(duration).await;
}

#[cfg(target_arch = "wasm32")]
pub(super) async fn sleep(duration: Duration) {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let global = js_sys::global();
        let timer = js_sys::Reflect::get(&global, &wasm_bindgen::JsValue::from_str("setTimeout"))
            .expect("host setTimeout");
        use wasm_bindgen::JsCast;
        let timer: js_sys::Function = timer.unchecked_into();
        timer
            .call2(
                &global,
                &resolve,
                &wasm_bindgen::JsValue::from_f64(duration.as_millis().min(i32::MAX as u128) as f64),
            )
            .expect("schedule timer");
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}
