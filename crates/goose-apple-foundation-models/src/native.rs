use super::{Error, ErrorKind, ModelInfo, Request, Response};
use std::ffi::{c_char, c_void, CStr, CString};
use tokio::sync::oneshot;

unsafe extern "C" {
    fn goose_afm_is_supported() -> bool;
    fn goose_afm_info() -> *mut c_char;
    fn goose_afm_free(value: *mut c_char);
    fn goose_afm_validate_schema(schema: *const c_char) -> *mut c_char;
    fn goose_afm_start(
        request: *const c_char,
        callback: extern "C" fn(*const c_char, *mut c_void),
        context: *mut c_void,
    ) -> *mut c_void;
    fn goose_afm_cancel(handle: *mut c_void);
}

pub(super) fn is_supported() -> bool {
    // This checks OS availability only, without touching the model or Swift tasks.
    unsafe { goose_afm_is_supported() }
}

#[derive(serde::Deserialize)]
struct Envelope<T> {
    result: Option<T>,
    error: Option<Error>,
}

fn decode<T: serde::de::DeserializeOwned>(json: &[u8]) -> Result<T, Error> {
    let envelope: Envelope<T> = serde_json::from_slice(json).map_err(|e| Error {
        kind: ErrorKind::Generation,
        message: e.to_string(),
    })?;
    match (envelope.result, envelope.error) {
        (_, Some(error)) => Err(error),
        (Some(result), None) => Ok(result),
        _ => Err(Error {
            kind: ErrorKind::Generation,
            message: "Empty native response".into(),
        }),
    }
}

pub(super) fn model_info() -> Result<ModelInfo, Error> {
    // Swift returns an owned, NUL-terminated allocation, freed by the matching allocator.
    unsafe {
        let pointer = goose_afm_info();
        let result = decode(CStr::from_ptr(pointer).to_bytes());
        goose_afm_free(pointer);
        result
    }
}

struct Generation(*mut c_void);
// The opaque handle is only used for cancellation. Swift Task.cancel is thread-safe.
unsafe impl Send for Generation {}
impl Drop for Generation {
    fn drop(&mut self) {
        // Consumes the retained Swift handle; the task owns its callback until completion.
        unsafe { goose_afm_cancel(self.0) }
    }
}

type Reply = oneshot::Sender<Result<Response, Error>>;
extern "C" fn completed(json: *const c_char, context: *mut c_void) {
    // Exactly one terminal callback consumes the box, including after cancellation.
    // No user code runs here and a dropped receiver is harmless.
    unsafe {
        let sender = Box::from_raw(context.cast::<Reply>());
        let _ = sender.send(decode(CStr::from_ptr(json).to_bytes()));
    }
}

pub(super) async fn generate(request: Request) -> Result<Response, Error> {
    let json =
        CString::new(serde_json::to_string(&request).map_err(|e| super::invalid(e.to_string()))?)
            .map_err(|e| super::invalid(e.to_string()))?;
    let (sender, receiver) = oneshot::channel();
    let context = Box::into_raw(Box::new(sender)).cast();
    // Swift copies the request before returning and guarantees one terminal callback.
    let _generation = Generation(unsafe { goose_afm_start(json.as_ptr(), completed, context) });
    receiver.await.map_err(|_| Error {
        kind: ErrorKind::Generation,
        message: "Native generation ended without a response".into(),
    })?
}

pub(super) fn validate_schema(schema: &str) -> Result<(), Error> {
    let schema = CString::new(schema).map_err(|e| super::invalid(e.to_string()))?;
    // The native validator copies the input and returns an owned JSON envelope.
    unsafe {
        let pointer = goose_afm_validate_schema(schema.as_ptr());
        let result = decode::<bool>(CStr::from_ptr(pointer).to_bytes());
        goose_afm_free(pointer);
        result.map(|_| ())
    }
}
