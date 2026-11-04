#[cfg(feature = "std")]
#[path = "canonical_std.rs"]
mod desktop;
#[cfg(feature = "std")]
pub use desktop::*;

#[cfg(not(feature = "std"))]
#[path = "canonical/model.rs"]
mod model;
#[cfg(not(feature = "std"))]
pub use model::*;
#[cfg(not(feature = "std"))]
pub fn maybe_get_canonical_model(_provider: &str, _model: &str) -> Option<CanonicalModel> {
    None
}

#[path = "canonical/setup.rs"]
pub mod setup;
#[cfg(not(feature = "std"))]
pub mod catalog {
    pub use super::setup::*;
}
#[cfg(not(feature = "std"))]
pub fn recommended_models_from_registry(_provider: &str) -> alloc::vec::Vec<alloc::string::String> {
    alloc::vec::Vec::new()
}
#[cfg(not(feature = "std"))]
pub fn map_to_canonical_model(
    _provider: &str,
    _model: &str,
    _registry: &CanonicalModelRegistry,
) -> Option<alloc::string::String> {
    None
}
#[cfg(not(feature = "std"))]
pub struct CanonicalModelRegistry;
#[cfg(not(feature = "std"))]
impl CanonicalModelRegistry {
    pub fn bundled() -> anyhow::Result<Self> {
        Ok(Self)
    }
    pub fn get(&self, _provider: &str, _model: &str) -> Option<&CanonicalModel> {
        None
    }
}
