use crate::model::ModelSettings;
use eredu_core::{
    AutomaticPlanningError, BackendId, DevicePlan, DraftPlacementPlan, DraftingPlan, ExecutionPlan,
    HardwareProfile,
};
use std::path::Path;

pub fn select_device(
    hardware: &HardwareProfile,
    backend: &BackendId,
    explicit: Option<&str>,
) -> Result<DevicePlan, AutomaticPlanningError> {
    let available = hardware
        .backends
        .iter()
        .find(|candidate| &candidate.backend == backend && candidate.available)
        .ok_or_else(|| {
            AutomaticPlanningError::Invalid(format!("Backend {backend} is unavailable"))
        })?;
    let selected = if let Some(explicit) = explicit {
        available
            .devices
            .iter()
            .find(|device| device.id == explicit)
    } else {
        available
            .devices
            .iter()
            .filter(|device| device.family != "cpu")
            .min_by_key(|device| (device.index, &device.id))
            .or_else(|| {
                available
                    .devices
                    .iter()
                    .filter(|device| device.family == "cpu")
                    .min_by_key(|device| device.index)
            })
    }
    .ok_or_else(|| {
        AutomaticPlanningError::Invalid(format!(
            "Requested device {} is unavailable for {backend}",
            explicit.unwrap_or("automatic")
        ))
    })?;
    DevicePlan::new(backend.to_string(), selected.id.clone())
        .map_err(|error| AutomaticPlanningError::Invalid(error.to_string()))
}

pub fn constrain_plan(
    plan: &ExecutionPlan,
    settings: &ModelSettings,
    draft: Option<&Path>,
) -> Result<ExecutionPlan, AutomaticPlanningError> {
    let mut plan = plan.clone();
    if let Some(maximum) = settings.max_cached_shards {
        plan = plan.with_max_cached_shards(maximum);
    }
    if let Some(draft) = draft {
        let max_draft_tokens = plan.drafting().max_draft_tokens().unwrap_or(3);
        plan = plan.with_drafting(DraftingPlan::External {
            model: draft.to_string_lossy().into_owned(),
            placement: DraftPlacementPlan::Target,
            max_draft_tokens,
            lookahead: true,
            adaptive_lookahead: true,
        });
    }
    Ok(plan)
}
