#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum ModelFitStatus {
    Recommended,
    Fits,
    TightFit,
    OverBudget,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum ModelFitEvidence {
    BackendRecommendation,
    SizeEstimate,
    Unknown,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct ModelFitAssessment {
    pub status: ModelFitStatus,
    pub required_bytes: Option<u64>,
    pub available_bytes: Option<u64>,
    pub effective_budget_bytes: Option<u64>,
    pub selection_blocked: bool,
    pub evidence: ModelFitEvidence,
    pub explanation: &'static str,
}

pub(super) fn assess_model_fit(
    required_bytes: Option<u64>,
    available_bytes: Option<u64>,
    recommended: Option<bool>,
    backend_no_fit: bool,
) -> ModelFitAssessment {
    let available_bytes = available_bytes.filter(|bytes| *bytes > 0);
    let required_bytes = required_bytes.filter(|bytes| *bytes > 0);
    let effective_budget_bytes = available_bytes.map(|bytes| bytes.saturating_mul(85) / 100);

    if backend_no_fit && available_bytes.is_some() {
        return ModelFitAssessment {
            status: ModelFitStatus::OverBudget,
            required_bytes,
            available_bytes,
            effective_budget_bytes,
            selection_blocked: true,
            evidence: ModelFitEvidence::BackendRecommendation,
            explanation: "Goose's inference runtime reports that no available variant fits its memory reserve.",
        };
    }

    if recommended == Some(true) {
        return ModelFitAssessment {
            status: ModelFitStatus::Recommended,
            required_bytes,
            available_bytes,
            effective_budget_bytes,
            selection_blocked: false,
            evidence: ModelFitEvidence::BackendRecommendation,
            explanation: "Recommended by Goose's local inference runtime.",
        };
    }

    let (Some(required_bytes), Some(available_bytes), Some(effective_budget_bytes)) =
        (required_bytes, available_bytes, effective_budget_bytes)
    else {
        return ModelFitAssessment {
            status: ModelFitStatus::Unknown,
            required_bytes,
            available_bytes,
            effective_budget_bytes,
            selection_blocked: false,
            evidence: ModelFitEvidence::Unknown,
            explanation: "Memory fit cannot be verified with the available model and runtime data.",
        };
    };

    let (status, explanation) = if required_bytes > available_bytes {
        (
            ModelFitStatus::OverBudget,
            "The reported model size exceeds available inference memory; this is an estimate, so selection remains allowed.",
        )
    } else if required_bytes > effective_budget_bytes {
        (
            ModelFitStatus::TightFit,
            "The reported model size leaves less than the runtime's 15% memory reserve.",
        )
    } else {
        (
            ModelFitStatus::Fits,
            "The reported model size is within the runtime's estimated memory budget.",
        )
    };

    ModelFitAssessment {
        status,
        required_bytes: Some(required_bytes),
        available_bytes: Some(available_bytes),
        effective_budget_bytes: Some(effective_budget_bytes),
        selection_blocked: false,
        evidence: ModelFitEvidence::SizeEstimate,
        explanation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_recommendation_is_marked_recommended() {
        let assessment = assess_model_fit(Some(6_000), Some(10_000), Some(true), false);

        assert_eq!(assessment.status, ModelFitStatus::Recommended);
        assert!(!assessment.selection_blocked);
    }

    #[test]
    fn size_that_fits_under_the_reserve_is_fits() {
        let assessment = assess_model_fit(Some(7_000), Some(10_000), Some(false), false);

        assert_eq!(assessment.status, ModelFitStatus::Fits);
        assert_eq!(assessment.effective_budget_bytes, Some(8_500));
        assert!(!assessment.selection_blocked);
    }

    #[test]
    fn size_above_reserve_but_below_available_memory_is_tight_fit() {
        let assessment = assess_model_fit(Some(9_000), Some(10_000), Some(false), false);

        assert_eq!(assessment.status, ModelFitStatus::TightFit);
        assert!(!assessment.selection_blocked);
    }

    #[test]
    fn backend_no_fit_result_blocks_selection() {
        let assessment = assess_model_fit(Some(9_000), Some(10_000), Some(false), true);

        assert_eq!(assessment.status, ModelFitStatus::OverBudget);
        assert!(assessment.selection_blocked);
    }

    #[test]
    fn missing_size_and_zero_memory_are_unknown_and_advisory() {
        let missing_size = assess_model_fit(None, Some(10_000), None, false);
        let zero_memory = assess_model_fit(Some(1_000), Some(0), None, false);

        assert_eq!(missing_size.status, ModelFitStatus::Unknown);
        assert_eq!(zero_memory.status, ModelFitStatus::Unknown);
        assert!(!missing_size.selection_blocked);
        assert!(!zero_memory.selection_blocked);
    }

    #[test]
    fn estimated_oversize_warns_without_blocking() {
        let assessment = assess_model_fit(Some(12_000), Some(10_000), None, false);

        assert_eq!(assessment.status, ModelFitStatus::OverBudget);
        assert_eq!(assessment.evidence, ModelFitEvidence::SizeEstimate);
        assert!(!assessment.selection_blocked);
    }
}
