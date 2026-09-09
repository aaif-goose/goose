// Exercises Eredu's speculative scheduler with deterministic portable execution.
use super::*;
struct MockSpeculativeExecutor;

impl SpeculativeExecutor for MockSpeculativeExecutor {
    type Input = Vec<u32>;
    type Cache = usize;
    type TargetState = ();
    type DraftState = ();
    type CacheCheckpoint = usize;
    type Verification = Vec<u32>;
    type Logits = u32;
    type Context<'a> = ();
    type Completion = Done;
    type Telemetry = ();
    type Error = io::Error;

    fn max_proposals(&self) -> usize {
        1
    }

    fn prefill<'a>(
        &mut self,
        input: Self::Input,
        cache: &mut Self::Cache,
        _: Self::Context<'a>,
    ) -> Result<SpeculativePrefill<Self::TargetState, Self::Logits>, Self::Error> {
        *cache = input.len();
        Ok(SpeculativePrefill::new(encode("a")[0], (), input.len()))
    }

    fn begin_proposal<'a>(
        &mut self,
        _: &Self::TargetState,
        _: u32,
        _: usize,
        _: Self::Context<'a>,
    ) -> Result<Self::DraftState, Self::Error> {
        Ok(())
    }

    fn proposal_logits<'a>(
        &mut self,
        _: &mut Self::DraftState,
        _: u32,
        _: Self::Context<'a>,
    ) -> Result<Self::Logits, Self::Error> {
        Ok(encode("b")[0])
    }

    fn checkpoint(&self, cache: &Self::Cache) -> Result<Self::CacheCheckpoint, Self::Error> {
        Ok(*cache)
    }

    fn restore_checkpoint<'a>(
        &mut self,
        cache: &mut Self::Cache,
        checkpoint: &Self::CacheCheckpoint,
        _: Self::Context<'a>,
    ) -> Result<(), Self::Error> {
        *cache = *checkpoint;
        Ok(())
    }

    fn submit_verification<'a>(
        &mut self,
        input_tokens: &[u32],
        cache: &mut Self::Cache,
        _: Self::Context<'a>,
    ) -> Result<Submission<Self::Verification, Self::Completion>, Self::Error> {
        *cache += input_tokens.len();
        Ok(Submission {
            output: vec![encode("b")[0], 1],
            completion: Done,
        })
    }

    fn verification_logits<'a>(
        &self,
        output: &Self::Verification,
        index: usize,
        _: Self::Context<'a>,
    ) -> Result<Self::Logits, Self::Error> {
        Ok(output[index])
    }

    fn commit_verification<'a>(
        &mut self,
        _: Self::Verification,
        _: Self::DraftState,
        cache: &mut Self::Cache,
        checkpoint: &Self::CacheCheckpoint,
        verified_inputs: usize,
        _: Self::Context<'a>,
    ) -> Result<SpeculativeCommit<Self::TargetState>, Self::Error> {
        *cache = *checkpoint + verified_inputs;
        Ok(SpeculativeCommit::new((), 0))
    }
}

#[derive(Clone)]
struct MockSpeculativeSampling;

impl SpeculativeSampling for MockSpeculativeSampling {
    type Logits = u32;
    type Distribution = u32;
    type Seed = ();
    type RandomState = ();
    type DraftRandomness = ();
    type RandomnessRoot = ();
    type Context<'a> = ();
    type Error = io::Error;

    fn randomness_root<'a>(
        _: Option<Self::Seed>,
        _: Self::Context<'a>,
    ) -> Result<Self::RandomnessRoot, Self::Error>
    where
        Self: 'a,
    {
        Ok(())
    }

    fn target_randomness_from_root<'a>(
        _: &mut Self::RandomnessRoot,
        _: Self::Context<'a>,
    ) -> Result<Self::RandomState, Self::Error>
    where
        Self: 'a,
    {
        Ok(())
    }

    fn draft_randomness_from_root<'a>(
        _: &mut Self::RandomnessRoot,
        _: Self::Context<'a>,
    ) -> Result<Self::DraftRandomness, Self::Error>
    where
        Self: 'a,
    {
        Ok(())
    }

    fn initialize_randomness<'a>(
        _: Option<Self::Seed>,
        _: f32,
        _: Self::Context<'a>,
    ) -> Result<SpeculativeRandomness<Self::RandomState, Self::DraftRandomness>, Self::Error>
    where
        Self: 'a,
    {
        Ok(SpeculativeRandomness::new(None, None))
    }

    fn draft_randomness_at<'a>(
        _: &Self::DraftRandomness,
        _: SpeculativeDraftRandomPosition,
        _: Self::Context<'a>,
    ) -> Result<Self::RandomState, Self::Error>
    where
        Self: 'a,
    {
        Ok(())
    }

    fn process_logits<'a>(
        &mut self,
        logits: &Self::Logits,
        _: f32,
        _: &[u32],
        _: SamplingPlacement,
        _: Self::Context<'a>,
    ) -> Result<Self::Distribution, Self::Error>
    where
        Self: 'a,
    {
        Ok(*logits)
    }

    fn sample<'a>(
        &self,
        distribution: &Self::Distribution,
        _: f32,
        _: Option<&mut Self::RandomState>,
        _: SamplingPlacement,
        _: Self::Context<'a>,
    ) -> Result<u32, Self::Error>
    where
        Self: 'a,
    {
        Ok(*distribution)
    }

    fn probability_at<'a>(
        &self,
        distribution: &Self::Distribution,
        token: u32,
        _: SamplingPlacement,
        _: Self::Context<'a>,
    ) -> Result<f32, Self::Error>
    where
        Self: 'a,
    {
        Ok(f32::from(*distribution == token))
    }

    fn sample_unit_interval<'a>(
        &self,
        _: Option<&mut Self::RandomState>,
        _: Self::Context<'a>,
    ) -> Result<f32, Self::Error>
    where
        Self: 'a,
    {
        Ok(0.0)
    }

    fn positive_probability_difference<'a>(
        &self,
        left: &Self::Distribution,
        _: &Self::Distribution,
        _: SamplingPlacement,
        _: Self::Context<'a>,
    ) -> Result<Option<Self::Distribution>, Self::Error>
    where
        Self: 'a,
    {
        Ok(Some(*left))
    }

    fn update_sampler_state<'a>(
        &mut self,
        _: &Self::Distribution,
        _: u32,
        _: SamplingPlacement,
        _: Self::Context<'a>,
    ) -> Result<(), Self::Error>
    where
        Self: 'a,
    {
        Ok(())
    }
}

impl SpeculativeGenerationBackend for MockBackend {
    type Drafter = ();

    fn speculative_capability(_: &ModelRuntime<Self>) -> SpeculativeCapability {
        SpeculativeCapability::Ready {
            draft_source: SpeculativeDraftSource::Embedded,
        }
    }

    fn with_speculative_execution<C, V>(
        runtime: &mut ModelRuntime<Self>,
        mut request: SpeculativeGenerationBatchRequest<'_, Self, Self::Drafter, C>,
        visitor: V,
    ) -> Result<SpeculativeGenerationBatchOutput, io::Error>
    where
        C: SpeculativeTokenFilterController,
        V: SpeculativeGenerationVisitor,
    {
        assert!(matches!(
            request.take_drafting(),
            SpeculativeDraft::Embedded
        ));
        let mut lanes = request.take_lanes();
        let mut caches = vec![0; lanes.len()];
        let mut prepared = Vec::with_capacity(lanes.len());
        for (mut lane, cache) in lanes.drain(..).zip(caches.iter_mut()) {
            assert!(!lane.prompt().is_empty());
            runtime
                .backend()
                .calls
                .lock()
                .unwrap()
                .configs
                .push(lane.take_generation());
            let constraint = lane.take_constraint();
            constraint.filter_at(&[]).unwrap();
            let config = lane.take_config();
            let sequence = eredu_core::generation::GenerationSequence::new(
                config.max_tokens,
                config.eos_token_ids.iter().copied(),
            );
            prepared.push(PreparedSpeculativeLane::new(
                cache,
                lane.take_prompt(),
                config,
                SpeculativeOutputRuntime::new(
                    MockSpeculativeSampling,
                    sequence,
                    SpeculativeSemanticConstraint::semantic(lane.take_semantic()),
                    SpeculativeCallbackPublisher::semantic(lane.take_on_event()),
                    lane.take_cancellation(),
                ),
                SpeculativeRandomness::new(None, None),
            ));
        }
        let output = visitor
            .run(
                &mut MockSpeculativeExecutor,
                prepared,
                eredu_core::SpeculativeExecutionTopology::Single,
                false,
                false,
                (),
            )
            .map_err(|error| io::Error::other(error.to_string()))?;
        Ok(output)
    }
}
