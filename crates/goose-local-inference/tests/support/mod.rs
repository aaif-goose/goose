use eredu_architectures::ModelKind;
use eredu_core::*;
use std::{
    io,
    num::NonZeroU8,
    path::Path,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, ThreadId},
};
use tokenizers::{models::bpe::BPE, AddedToken, Tokenizer};
mod artifact;
mod contracts;
pub use artifact::write_artifact;
pub const TEMPLATE: &str = include_str!("qwen.jinja");

#[derive(Default)]
pub struct Calls {
    pub loads: usize,
    pub resets: usize,
    pub settles: usize,
    pub drops: usize,
    pub threads: Vec<ThreadId>,
    pub configs: Vec<TextGenerationConfig>,
    pub prompts: Vec<Vec<u32>>,
    pub admitted: Vec<ExecutionPlan>,
    pub realized: Vec<ExecutionPlan>,
}

#[derive(Clone)]
pub struct MockBackend {
    pub calls: Arc<Mutex<Calls>>,
    pub output: Vec<u32>,
    pub model_bytes: u64,
    pub host_bytes: u64,
    pub device_bytes: u64,
    pub fail_settle: Arc<AtomicBool>,
    pub panic_generate: Arc<AtomicBool>,
    pub delay: bool,
    pub embedded_draft: bool,
    owner: Rc<ThreadId>,
}

impl MockBackend {
    pub fn new(calls: Arc<Mutex<Calls>>, output: Vec<u32>) -> Self {
        Self {
            calls,
            output,
            model_bytes: 1024,
            host_bytes: 64 * 1024,
            device_bytes: 8 * 1024,
            fail_settle: Arc::default(),
            panic_generate: Arc::default(),
            delay: false,
            embedded_draft: false,
            owner: Rc::new(thread::current().id()),
        }
    }
    fn assert_owner(&self) {
        assert_eq!(*self.owner, thread::current().id());
    }
    fn submit(&self, position: &mut usize) -> io::Result<Submission<Token, Done>> {
        self.assert_owner();
        assert!(
            !self.panic_generate.load(Ordering::Relaxed),
            "injected worker panic"
        );
        if self.delay {
            thread::sleep(std::time::Duration::from_millis(10));
        }
        let id = self.output.get(*position).copied().unwrap_or(1);
        *position += 1;
        Ok(Submission {
            output: Token(id),
            completion: Done,
        })
    }
}

pub struct Session {
    calls: Arc<Mutex<Calls>>,
    owner: Rc<ThreadId>,
}
impl Drop for Session {
    fn drop(&mut self) {
        assert_eq!(*self.owner, thread::current().id());
        self.calls.lock().unwrap().drops += 1;
    }
}
pub struct Done;
#[derive(Clone)]
pub struct Token(u32);
impl TokenOutput for Token {
    type Error = io::Error;
    fn token_id(&self) -> io::Result<u32> {
        Ok(self.0)
    }
}
impl Completion for Done {
    type Error = io::Error;
    fn is_complete(&self) -> io::Result<bool> {
        Ok(true)
    }
    fn wait(&self) -> io::Result<()> {
        Ok(())
    }
}
impl BoundedCompletion for Done {
    fn wait_bounded(self, _: BoundedCompletionWait) -> io::Result<BoundedCompletionOutcome> {
        Ok(BoundedCompletionOutcome::Completed)
    }
}
impl BackendProvider for MockBackend {
    type ModelConfig = ();
    type Model = ();
    type Session = Session;
    type Error = io::Error;
    fn descriptor(&self) -> BackendDescriptor {
        BackendDescriptor::new("mock", "test")
    }
    fn devices(&self) -> io::Result<Vec<(DeviceDescriptor, DeviceCapabilities)>> {
        Ok(vec![(
            DeviceDescriptor::new("gpu:0", "mock", "mock-accelerator", Some(self.device_bytes)),
            DeviceCapabilities::new(true, true, true),
        )])
    }
    fn prepare_model(&self, _: ()) -> io::Result<PreparedModel<()>> {
        self.assert_owner();
        let mut calls = self.calls.lock().unwrap();
        calls.loads += 1;
        calls.threads.push(thread::current().id());
        Ok(PreparedModel::new(
            (),
            SessionCapabilities::new(true, true, false),
        ))
    }
    fn create_session(&self, _: PreparedModel<()>) -> io::Result<Session> {
        Ok(Session {
            calls: self.calls.clone(),
            owner: self.owner.clone(),
        })
    }
}
impl BackendSession<MockBackend> for Session {
    type PrefillInput = Vec<u32>;
    type DecodeInput = u32;
    type Output = u32;
    type Completion = Done;
    fn capabilities(&self) -> SessionCapabilities {
        SessionCapabilities::new(true, true, false)
    }
    fn prefill(&mut self, _: &MockBackend, input: Vec<u32>) -> io::Result<Submission<u32, Done>> {
        Ok(Submission {
            output: input.len() as u32,
            completion: Done,
        })
    }
    fn decode(&mut self, _: &MockBackend, input: u32) -> io::Result<Submission<u32, Done>> {
        Ok(Submission {
            output: input,
            completion: Done,
        })
    }
    fn observe_output(&self, _: &MockBackend, _: &u32) -> io::Result<ObservationSet> {
        Ok(ObservationSet::new())
    }
}
impl TextGenerationBackend for MockBackend {
    type Prompt = Vec<u32>;
    type Token = Token;
    type TextGenerationState = usize;
    type TextCompletion = Done;
    fn text_execution_control_support(
        _: &ModelRuntime<Self>,
    ) -> eredu_core::execution_control::ControlSupport {
        eredu_core::execution_control::ControlSupport::Supported
    }
    fn reset_session(backend: &Self, _: &mut Session) -> Result<(), BackendFailure> {
        backend.assert_owner();
        backend.calls.lock().unwrap().resets += 1;
        Ok(())
    }
    fn synchronize_session(backend: &Self, _: &Session) -> Result<(), BackendFailure> {
        backend.assert_owner();
        backend.calls.lock().unwrap().settles += 1;
        if backend.fail_settle.load(Ordering::Relaxed) {
            Err(BackendFailure::from_error(io::Error::other(
                "injected settlement failure",
            )))
        } else {
            Ok(())
        }
    }
    fn start_text_generation(backend: &Self, config: TextGenerationConfig) -> io::Result<usize> {
        backend.calls.lock().unwrap().configs.push(config);
        Ok(0)
    }
    fn prepare_text_prompt(backend: &Self, prompt: Vec<u32>) -> io::Result<Vec<u32>> {
        backend.calls.lock().unwrap().prompts.push(prompt.clone());
        Ok(prompt)
    }
    fn submit_text_prefill(
        runtime: &mut ModelRuntime<Self>,
        _: Vec<u32>,
        _: &TokenFilter,
        state: &mut usize,
    ) -> io::Result<Submission<Token, Done>> {
        runtime.backend().submit(state)
    }
    fn submit_text_decode(
        runtime: &mut ModelRuntime<Self>,
        _: Token,
        _: &TokenFilter,
        state: &mut usize,
    ) -> io::Result<Submission<Token, Done>> {
        runtime.backend().submit(state)
    }
}
mod speculative;

pub fn tokenizer() -> Tokenizer {
    let special = ["[UNK]", "<|im_end|>", "<|im_start|>"];
    let mut vocabulary: std::collections::HashMap<String, u32> = special
        .iter()
        .enumerate()
        .map(|(index, token)| (token.to_string(), index as u32))
        .collect();
    let mut alphabet: Vec<_> = tokenizers::pre_tokenizers::byte_level::ByteLevel::alphabet()
        .into_iter()
        .collect();
    alphabet.sort();
    for character in alphabet {
        vocabulary.insert(character.to_string(), vocabulary.len() as u32);
    }
    let model = BPE::builder()
        .vocab_and_merges(
            vocabulary
                .into_iter()
                .collect::<tokenizers::models::bpe::Vocab>(),
            vec![],
        )
        .unk_token("[UNK]".into())
        .build()
        .unwrap();
    let mut tokenizer = Tokenizer::new(model);
    tokenizer.with_pre_tokenizer(Some(
        tokenizers::pre_tokenizers::byte_level::ByteLevel::new(false, false, false),
    ));
    tokenizer.with_decoder(Some(tokenizers::decoders::byte_level::ByteLevel::default()));
    tokenizer
        .add_special_tokens(
            special
                .into_iter()
                .map(|token| AddedToken::from(token, true).normalized(false)),
        )
        .unwrap();
    tokenizer
}

pub fn encode(text: &str) -> Vec<u32> {
    tokenizer().encode(text, false).unwrap().get_ids().to_vec()
}
