use std::{
    error::Error,
    fmt::Display,
    sync::{Arc, OnceLock},
};

use serde::Serialize;
use serde_json::json;

use crate::conversation::token_usage::Usage;

type RequestLogError = Box<dyn Error + Send + Sync>;

static LOGGER: OnceLock<Arc<dyn RequestLogger>> = OnceLock::new();

#[derive(Debug)]
pub struct LoggerAlreadyInstalled;

impl Display for LoggerAlreadyInstalled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "request logger is already installed")
    }
}

impl Error for LoggerAlreadyInstalled {}

pub fn install_logger<R: RequestLogger + 'static>(r: R) -> Result<(), LoggerAlreadyInstalled> {
    LOGGER.set(Arc::new(r)).map_err(|_| LoggerAlreadyInstalled)
}

pub trait RequestLogger: Send + Sync {
    fn start(&self) -> Result<Box<dyn RequestLogHandle>, RequestLogError>;
}

pub trait RequestLogHandle: Send {
    fn write(&mut self, s: &str) -> Result<(), RequestLogError>;
}

#[derive(Debug)]
pub enum LogError {
    LoggerError(String),
    SerializeError(serde_json::Error),
}

impl Display for LogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogError::LoggerError(msg) => write!(f, "{}", msg),
            LogError::SerializeError(error) => write!(f, "serialize error: {}", error),
        }
    }
}

impl Error for LogError {}

impl From<RequestLogError> for LogError {
    fn from(value: RequestLogError) -> Self {
        Self::LoggerError(value.to_string())
    }
}

fn serialize(v: &serde_json::Value) -> Result<String, LogError> {
    serde_json::to_string(v).map_err(LogError::SerializeError)
}

pub fn start_log<M, P>(
    model_config: M,
    payload: P,
) -> Result<Option<Box<dyn RequestLogHandle>>, LogError>
where
    M: Serialize,
    P: Serialize,
{
    let logger = if let Some(logger) = LOGGER.get() {
        logger
    } else {
        return Ok(None);
    };

    let mut handle = logger.start()?;
    let payload = json!({
        "model_config": model_config,
        "input": payload,
    });

    handle.write(serialize(&payload)?.as_str())?;
    Ok(Some(handle))
}

pub trait LoggerHandleExt {
    fn write<Payload>(&mut self, data: &Payload, usage: Option<&Usage>) -> Result<(), LogError>
    where
        Payload: Serialize;
    fn error<E>(&mut self, error: E) -> Result<(), LogError>
    where
        E: Display;
}

impl LoggerHandleExt for Option<Box<dyn RequestLogHandle>> {
    fn write<Payload>(&mut self, data: &Payload, usage: Option<&Usage>) -> Result<(), LogError>
    where
        Payload: Serialize,
    {
        let log = if let Some(log) = self {
            log
        } else {
            return Ok(());
        };

        let line = serialize(&json!({
            "data": data,
            "usage": usage,
        }))?;

        Ok(log.write(line.as_str())?)
    }

    fn error<E>(&mut self, error: E) -> Result<(), LogError>
    where
        E: Display,
    {
        let log = if let Some(log) = self {
            log
        } else {
            return Ok(());
        };

        let line = serialize(&json!({
            "error": format!("{}", error),
        }))?;

        Ok(log.write(line.as_str())?)
    }
}
