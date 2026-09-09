use super::{
    error,
    generation::{generate, Request},
    planning::{constrain_plan, select_device},
    settings::text_options,
};
use crate::{model::ModelSettings, StreamSender};
use eredu::api::{inspect_text_model, LoadedModel, TextInspectionOptions};
use eredu_architectures::{processor_plan::ArtifactArchitecturePlan, ExternalAssistantPreparation};
use eredu_core::{
    ArtifactInspection, AutomaticPlanRequest, AutomaticPlanner, AutomaticPlanningBackend,
    ExecutionPlanBackendFactory, ExecutionPlanReport, GenerationCancellationToken,
    ModelCapabilityBackend, ModelConfigurationResolver, ModelInspectionReport, ModelLoadingBackend,
    SpeculativeGenerationBackend,
};
use goose_provider_types::{conversation::token_usage::ProviderUsage, errors::ProviderError};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{mpsc, Mutex, OnceLock},
    thread,
    time::Duration,
};

static TELEMETRY: OnceLock<Mutex<VecDeque<(String, eredu_core::ExecutionTelemetry)>>> =
    OnceLock::new();

struct Command {
    request: Request,
    events: StreamSender,
    cancellation: GenerationCancellationToken,
    reply: mpsc::SyncSender<Result<ProviderUsage, ProviderError>>,
}

pub struct WorkerHandle {
    commands: Option<mpsc::SyncSender<Command>>,
    thread: Option<thread::JoinHandle<()>>,
    pub report: ExecutionPlanReport,
    pub inspection: ModelInspectionReport,
}

impl WorkerHandle {
    pub fn spawn<F, Make>(
        make_factory: Make,
        path: PathBuf,
        settings: ModelSettings,
        draft: Option<PathBuf>,
    ) -> Result<Self, ProviderError>
    where
        Make: FnOnce() -> F + Send + 'static,
        F: ExecutionPlanBackendFactory<DrafterPreparation = ExternalAssistantPreparation>
            + AutomaticPlanningBackend<Inspection = ArtifactInspection<ArtifactArchitecturePlan>>,
        F::Backend: ModelCapabilityBackend + SpeculativeGenerationBackend<Drafter = F::Drafter>,
        <F::Backend as ModelLoadingBackend>::ConfigurationResolver:
            ModelConfigurationResolver<ArtifactPlan = ArtifactArchitecturePlan>,
    {
        let (commands, receiver) = mpsc::sync_channel::<Command>(1);
        let (ready, initialization) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("goose-eredu".into())
            .spawn(move || {
                let factory = make_factory();
                let identity = serde_json::json!({"backend": factory.backend_id(), "path": path, "draft": draft, "template": settings.chat_template, "device": settings.device, "max_cached_shards": settings.max_cached_shards}).to_string();
                let prior = TELEMETRY.get_or_init(Default::default).lock().unwrap().iter().filter(|(key,_)| key == &identity).map(|(_,value)| value.clone()).collect::<Vec<_>>();
                let load = || {
                    let hardware = factory.discover_hardware().map_err(error)?;
                    let device =
                        select_device(&hardware, &factory.backend_id(), settings.device.as_deref())
                            .map_err(error)?;
                    let request = AutomaticPlanRequest::new(&path, device).with_prior_telemetry(prior.clone());
                    let retained = AutomaticPlanner::default()
                        .plan_retained_with_overrides(&factory, &request, |plan, _| {
                            constrain_plan(plan, &settings, draft.as_deref())
                        })
                        .map_err(error)?;
                    let (report, artifact) = retained.into_parts();
                    let options = text_options(&settings.chat_template)?;
                    let mut inspection =
                        ModelInspectionReport::unverified(artifact.path(), artifact.format());
                    inspection.record_artifact_inspection(&artifact);
                    let inspection =
                        inspect_text_model(inspection, &options, TextInspectionOptions::default());
                    let model = LoadedModel::load_inspected_execution_plan_with_text_options(
                        &factory,
                        artifact,
                        &report.plan,
                        options,
                    )
                    .map_err(error)?;
                    Ok::<_, ProviderError>((model, report, inspection))
                };
                let (mut planned, report, inspection) = match load() {
                    Ok(loaded) => loaded,
                    Err(err) => {
                        let _ = ready.send(Err(err));
                        return;
                    }
                };
                if ready.send(Ok((report.clone(), inspection))).is_err() {
                    let _ = planned.model().synchronize();
                    return;
                }
                for command in receiver {
                    let reset = planned.model_mut().reset().map_err(error);
                    if let Err(err) = reset {
                        let _ = command.reply.send(Err(err));
                        break;
                    }
                    let result = generate(
                        &mut planned,
                        &command.request,
                        &command.events,
                        command.cancellation,
                    );
                    let settled = planned.model().synchronize().map_err(error);
                    let healthy = settled.is_ok();
                    let result = settled.and(result);
                    if let Ok(usage) = &result {
                        let stats = usage.stats.as_ref().expect("adapter provides generation stats");
                        let load = Duration::from_millis(stats.model_load_ms.unwrap_or(0));
                        let elapsed = Duration::from_millis(stats.elapsed_ms.unwrap_or(0));
                        let telemetry = eredu_core::ExecutionTelemetry {
                            schema_version: eredu_core::AUTOMATIC_SCHEMA_VERSION,
                            effective_model_type: planned.model().effective_model_type().to_owned(),
                            plan: Some(report.plan.clone()), plan_explanation: Some(report.explanation.clone()),
                            hardware: Some(report.hardware.clone()), resources: Some(report.resources.clone()),
                            prompt_tokens: usage.usage.input_tokens.unwrap_or(0) as usize,
                            generated_tokens: stats.output_tokens.unwrap_or(0),
                            stop_reason: usage.finish_reasons.as_ref().and_then(|reasons| reasons.first()).cloned().unwrap_or_default(),
                            timing: eredu_core::TimingTelemetry::new(load, elapsed, stats.time_to_first_token_ms.map(Duration::from_millis), stats.output_tokens.unwrap_or(0), load + elapsed),
                            allocator: None, residency: None, expert_cache: None,
                            speculative: usage.additional_data.as_ref().and_then(|data| data.get("eredu_speculation")).map(|value| serde_json::from_value(value.clone()).expect("adapter serializes speculative telemetry")),
                        };
                        let mut history = TELEMETRY.get().unwrap().lock().unwrap();
                        history.retain(|(key,_)| key != &identity);
                        history.push_back((identity.clone(), telemetry));
                        if history.len() > 32 { history.pop_front(); }
                    }
                    let _ = command.reply.send(result);
                    if !healthy {
                        break;
                    }
                }
                if let Err(err) = planned.model().synchronize() {
                    tracing::error!(%err, "Eredu worker could not settle before shutdown");
                }
            })
            .map_err(error)?;
        match initialization
            .recv()
            .map_err(|_| error("model worker exited during loading"))?
        {
            Ok((report, inspection)) => Ok(Self {
                commands: Some(commands),
                thread: Some(thread),
                report,
                inspection,
            }),
            Err(err) => {
                let _ = thread.join();
                Err(err)
            }
        }
    }

    pub fn generate(
        &self,
        request: Request,
        events: StreamSender,
        cancellation: GenerationCancellationToken,
    ) -> Result<ProviderUsage, ProviderError> {
        let (reply, result) = mpsc::sync_channel(1);
        self.commands
            .as_ref()
            .ok_or_else(|| error("model worker is shut down"))?
            .send(Command {
                request,
                events,
                cancellation,
                reply,
            })
            .map_err(|_| error("model worker exited"))?;
        result
            .recv()
            .map_err(|_| error("model worker failed during generation"))?
    }
}

impl Drop for WorkerHandle {
    fn drop(&mut self) {
        self.commands.take();
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                tracing::error!("Eredu model worker panicked");
            }
        }
    }
}
