//! Goose application translation around Eredu's portable runtime contracts.
pub mod generation;
pub mod planning;
pub mod settings;
mod worker;
pub use worker::WorkerHandle;

pub fn error(error: impl std::fmt::Display) -> goose_provider_types::errors::ProviderError {
    goose_provider_types::errors::ProviderError::ExecutionError(format!("Eredu: {error}"))
}
