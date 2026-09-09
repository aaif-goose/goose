// Portable backend contracts adapted from Eredu b4ef9f32 backend_conformance.rs.
use super::*;
impl ModelCapabilityBackend for MockBackend {
    fn model_capabilities(_: &ModelRuntime<Self>) -> Result<ModelCapabilities, CapabilityError> {
        Ok(ModelCapabilities {
            effective_model_type: "mistral".into(),
            native_max_context: Observed::exact(8192, "mock configuration"),
            effective_max_context: Observed::exact(8192, "mock configuration"),
            state_strategy: CacheStateStrategy::FullKv,
            modalities: InputModalities::TEXT,
            estimation: EstimationCompleteness::Complete,
        })
    }

    fn count_prepared_input(
        _: &ModelRuntime<Self>,
        input: &Self::Prompt,
    ) -> Result<InputTokenCount, CapabilityError> {
        Ok(InputTokenCount::text(input.len() as u64))
    }

    fn estimate_runtime_state(
        _: &ModelRuntime<Self>,
        input: InputTokenCount,
        max_output_tokens: u64,
        batch_size: u64,
    ) -> Result<RuntimeStateEstimate, CapabilityError> {
        eredu_core::estimate_runtime_state(
            &StateMemoryLayout::new(
                eredu_core::LayerSchedule::new(
                    1,
                    vec![eredu_core::cache::LayerCachePolicy::key_only(
                        eredu_core::AttentionPolicy::Full,
                        1,
                        2,
                    )
                    .unwrap()],
                )
                .unwrap(),
                vec![0],
                1,
                1,
                EstimationCompleteness::Complete,
            )
            .unwrap(),
            input,
            max_output_tokens,
            batch_size,
            NonZeroU8::new(4).unwrap(),
        )
    }

    fn static_memory(_: &ModelRuntime<Self>) -> Result<StaticMemoryReport, CapabilityError> {
        let unavailable = || Observed::unavailable("mock counter is unavailable");
        Ok(StaticMemoryReport {
            logical_parameter_bytes: Observed::exact(128, "mock model"),
            current_host_resident_bytes: unavailable(),
            current_device_resident_bytes: unavailable(),
            planned_disk_backed_bytes: unavailable(),
            backend_active_allocation_bytes: unavailable(),
            backend_allocator_cache_bytes: unavailable(),
            physical_semantics: PhysicalMemorySemantics::Unknown,
            currently_cached_shards: unavailable(),
        })
    }
}

impl ModelLoadingBackend for MockBackend {
    type LoadOptions = ();
    type SelectedPreparation = eredu_core::PreparationAdmission;
    type ConfigurationResolver = eredu_architectures::configuration::ModelConfigurations;

    fn configuration_resolver(&self) -> &Self::ConfigurationResolver {
        &eredu_architectures::configuration::MODEL_CONFIGURATIONS
    }

    fn select_preparation(
        &self,
        inspection: &eredu_core::ArtifactInspection<
            eredu_architectures::processor_plan::ArtifactArchitecturePlan,
        >,
        _: &Self::LoadOptions,
    ) -> Result<Self::SelectedPreparation, Self::Error> {
        let request = eredu_core::PreparationAdmissionRequest::new(
            eredu_core::LoadingProtocol::Model,
            inspection.format(),
            eredu_core::PreparationPolicy::default(),
            eredu_core::ArchitecturePreparationCapabilities::new(
                false,
                true,
                false,
                false,
                false,
                eredu_core::InputModalities::TEXT,
            ),
        );
        Ok(eredu_core::admit_preparation(
            request,
            eredu_core::PreparationMechanismCapabilities::new(true, true)
                .with_residency(eredu_core::ResidencyRequest::FullyResident, true)
                .with_input_modalities(eredu_core::InputModalities::TEXT)
                .with_session(SessionCapabilities::new(true, true, false)),
        )
        .expect("mock preparation facts are coherent"))
    }

    fn selected_preparation_admission(
        &self,
        selected: &Self::SelectedPreparation,
    ) -> eredu_core::PreparationAdmission {
        *selected
    }

    fn model_config(
        &self,
        selected: eredu_core::SelectedModelPreparation<Self>,
    ) -> Result<Self::ModelConfig, Self::Error> {
        let (plan, _admission) = selected.into_parts();
        assert_eq!(plan.inspection().configuration().family(), "llama");
        Ok(())
    }
}

impl AutomaticPlanningBackend for MockBackend {
    type Inspection = eredu_core::ArtifactInspection<
        eredu_architectures::processor_plan::ArtifactArchitecturePlan,
    >;

    fn backend_id(&self) -> BackendId {
        BackendId::new("mock").unwrap()
    }

    fn discover_hardware(&self) -> Result<HardwareProfile, AutomaticPlanningError> {
        Ok(HardwareProfile {
            schema_version: AUTOMATIC_SCHEMA_VERSION,
            operating_system: "portable-test".into(),
            architecture: "mock".into(),
            logical_cpu_count: Observed::exact(4, "conformance fixture"),
            physical_memory_bytes: Observed::exact(self.host_bytes, "conformance fixture"),
            available_memory_bytes: Observed::exact(self.host_bytes, "conformance fixture"),
            physical_memory_semantics: HardwareMemorySemantics::SeparateTiers,
            backends: vec![HardwareBackendProfile {
                backend: self.backend_id(),
                available: true,
                detail: None,
                devices: vec![HardwareDeviceProfile {
                    id: "gpu:0".into(),
                    family: "mock-accelerator".into(),
                    index: 0,
                    total_memory_bytes: Observed::exact(self.device_bytes, "conformance fixture"),
                    available_memory_bytes: Observed::exact(
                        self.device_bytes,
                        "conformance fixture",
                    ),
                }],
            }],
        })
    }

    fn inspect_resources(
        &self,
        model_path: &Path,
    ) -> Result<(ModelResourceProfile, Self::Inspection), AutomaticPlanningError> {
        assert!(model_path.join("model.safetensors").is_file());
        let inspection = eredu_architectures::configuration::inspect_artifact(model_path)
            .map_err(|error| AutomaticPlanningError::Invalid(error.to_string()))?;
        let mut profile =
            ModelResourceProfile::unmeasured(model_path.into(), ArtifactFormat::SafeTensors);
        profile.model_family = Some(ModelKind::Llama.canonical_name().into());
        profile.architecture = Some("mistral".into());
        profile.tensor_count = Some(1);
        profile.checkpoint_shards = Some(1);
        profile.embedded_draft_layers = Observed::exact(
            usize::from(self.embedded_draft),
            "normalized architecture fixture",
        );
        profile.embedded_draft_capacity =
            Observed::exact(2 * usize::from(self.embedded_draft), "fixture");
        profile.stored_tensor_bytes = Observed::exact(4, "conformance fixture");
        profile.largest_stored_tensor_bytes = Observed::exact(4, "conformance fixture");
        profile.materialized_parameter_bytes =
            Observed::exact(self.model_bytes, "conformance fixture");
        Ok((profile, inspection))
    }

    fn admit_candidate(
        &self,
        _: &Self::Inspection,
        plan: &ExecutionPlan,
    ) -> Result<CandidateAdmission, AutomaticPlanningError> {
        self.calls.lock().unwrap().admitted.push(plan.clone());
        assert_eq!(plan.device().backend(), &self.backend_id());
        let supported = plan.expert_cache().is_none();
        Ok(CandidateAdmission {
            supported,
            rejection: (!supported).then(|| "mock model has no routed experts".into()),
        })
    }

    fn bounded_residency_requirement(
        &self,
        _: &Self::Inspection,
        plan: &ExecutionPlan,
    ) -> Result<BoundedResidencyRequirement, AutomaticPlanningError> {
        assert!(matches!(
            plan.residency(),
            ResidencyPlan::LayerwiseHost { .. } | ResidencyPlan::DenseDiskStream { .. }
        ));
        Ok(BoundedResidencyRequirement {
            static_bytes: 1024,
            window_bytes: 2048,
            required_bytes: 3072,
            depth: 1,
        })
    }
}

impl ExecutionPlanBackendFactory for MockBackend {
    type Backend = Self;
    type DrafterPreparation = eredu_architectures::ExternalAssistantPreparation;
    type SelectedDrafterPreparation = eredu_architectures::ExternalAssistantPreparation;
    type Drafter = ();

    fn select_target(
        &self,
        inspection: &eredu_core::ArtifactInspection<
            eredu_architectures::processor_plan::ArtifactArchitecturePlan,
        >,
        _: &ExecutionPlan,
    ) -> Result<ExecutionPlanTargetSelection<Self::Backend>, AutomaticPlanningError> {
        Ok(ExecutionPlanTargetSelection::new(
            eredu_core::PreparationPolicy::default(),
            self.select_preparation(inspection, &())
                .expect("mock target selection is coherent"),
            SessionCapabilities::new(true, true, false),
        ))
    }

    fn realize_target(
        &self,
        selected: SelectedExecutionPlanTarget<Self::Backend>,
    ) -> Result<ExecutionPlanTarget<Self::Backend>, AutomaticPlanningError> {
        self.calls
            .lock()
            .unwrap()
            .realized
            .push(selected.execution_plan().clone());
        Ok(ExecutionPlanTarget::new(self.clone(), selected))
    }

    fn select_drafting(
        &self,
        _: &ExecutionPlan,
        _: &SelectedExecutionPlanTarget<Self::Backend>,
        external_artifact: Option<ExternalDraftArtifact<Self::DrafterPreparation>>,
    ) -> Result<
        Option<ExternalDraftArtifact<Self::SelectedDrafterPreparation>>,
        AutomaticPlanningError,
    > {
        Ok(external_artifact)
    }

    fn realize_drafting(
        &self,
        plan: &ExecutionPlan,
        target: &ModelRuntime<Self::Backend>,
        selected: eredu_core::SelectedExecutionPlanDrafting<Self::SelectedDrafterPreparation>,
    ) -> Result<RealizedDrafting<()>, AutomaticPlanningError> {
        let external_artifact = selected.into_external_artifact(plan, target)?;
        Ok(match plan.drafting() {
            DraftingPlan::Disabled => {
                assert!(external_artifact.is_none());
                RealizedDrafting::Disabled
            }
            DraftingPlan::Embedded { .. } => {
                assert!(external_artifact.is_none());
                RealizedDrafting::Embedded
            }
            DraftingPlan::External { .. } => {
                let artifact = external_artifact.expect("external drafting carries identities");
                let _shared_tokenizer_fingerprint = artifact.tokenizer_compatibility.fingerprint();
                RealizedDrafting::External(())
            }
            _ => {
                return Err(AutomaticPlanningError::Invalid(
                    "unsupported drafting plan".into(),
                ))
            }
        })
    }
}
