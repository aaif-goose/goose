---
title: Hello World
sidebar_label: Getting Started
description: Build a small Rust agent with one shell tool, streaming responses, and a saved conversation.
displayed_sidebar: gdkSidebar
---

# Hello World

Let's build a small coding agent. It can inspect a directory, run shell
commands, and keep a conversation across turns and application restarts.

This is an annotated walkthrough of Jack's
[`ngrc` example](https://github.com/jamadeo/ngrc/tree/dc25c579cd40d85f8d90b61e80cbf02b8838564f).
GDK handles inference, tool dispatch, and the model/tool loop. The application
chooses the provider, tools, prompt, storage, and interface.

The code blocks below are selected excerpts, with some setup shortened for
clarity. Use the linked repository for the complete runnable program, imports,
and session store. This walkthrough uses revision `dc25c57`, which pins GDK
`0.1.0-alpha.12`; newer alpha releases may have different APIs.

## Run it first

You need Rust 1.94.1 or later, macOS or Linux with `/bin/bash`, and an OpenAI
API key. Choose a model that supports tool calling. You do not need the goose
CLI, a goose configuration file, or an MCP server.

:::warning Shell access
This example executes shell commands without approval or sandboxing, with your
user permissions and inherited environment. Use a disposable workspace without
sensitive files or credentials. Choosing a working directory does not restrict
what the shell can access. Model requests may incur charges.
:::

```bash
git clone https://github.com/jamadeo/ngrc.git
cd ngrc
git switch --detach dc25c579cd40d85f8d90b61e80cbf02b8838564f
```

Set these to your own API key and a tool-capable OpenAI model available to your
account, then send an initial prompt:

```bash
export OPENAI_API_KEY="your-api-key"
export NGRC_MODEL="your-tool-capable-openai-model"
cargo run --locked -- "Inspect this directory and summarize it without changing files."
```

The agent prints its response and returns to a `>` prompt. Ask a follow-up
question to use the same conversation. Ctrl-D on an empty line exits; Ctrl-C
cancels the agent and exits. Starting without a prompt waits for your first
message rather than making a model request immediately.

The application also prints the path to its session file. Keep that path to
[resume the conversation](#resume-the-conversation) later.

## Assemble the agent

The central function in
[`src/main.rs`](https://github.com/jamadeo/ngrc/blob/dc25c579cd40d85f8d90b61e80cbf02b8838564f/src/main.rs)
is small enough to read in one go:

```rust
fn agent(
    provider: Arc<dyn Provider>,
    model: ModelConfig,
    cancel: CancellationToken,
) -> StateMachine<'static, Session, Effect> {
    StateMachine::new(
        vec![
            Step::Operation(Arc::new(
                ToolOperation::<Session>::new().with_async_tool::<Shell>(),
            )),
            Step::Inference(Arc::new(
                InferenceRunner::<Session, Effect>::new(provider, model)
                    .with_request_preparer(Arc::new(Prompt)),
            )),
        ],
        cancel,
    )
}
```

The machine has two steps, in order:

- `ToolOperation` exposes the shell tool to the model and executes pending
  calls to it.
- `InferenceRunner` calls the provider with the conversation, prompt, and
  available tools.

After a step produces effects, the runtime saves them and the machine reloads
the session before the next pass. A model response can request a shell command;
the next pass executes it, and another inference call sees the result. When
the assistant finishes its turn, control returns to the application.

There is no second, application-written model/tool loop hidden around this
function. The application only needs a loop for accepting new user messages.

## Choose a provider and model

The example uses three GDK crates:

```toml
[dependencies]
goose-agent = "=0.1.0-alpha.12"
goose-provider-types = "=0.1.0-alpha.12"
goose-providers = { version = "=0.1.0-alpha.12", features = ["rustls-tls"] }
```

`goose-agent` provides the state machine and operations. `goose-provider-types`
provides shared interfaces and message types. `goose-providers` supplies the
OpenAI implementation. The `rustls-tls` feature enables HTTPS for this setup.
The [full manifest](https://github.com/jamadeo/ngrc/blob/dc25c579cd40d85f8d90b61e80cbf02b8838564f/Cargo.toml)
also includes Tokio, serialization, and the typed tool dependencies.

Provider setup is explicit:

```rust
let key = std::env::var("OPENAI_API_KEY")?;
let model_name = std::env::var("NGRC_MODEL")?;

let client = ApiClient::new_with_tls(
    "https://api.openai.com".into(),
    AuthMethod::BearerToken(key),
    Some(Default::default()),
)?;

let provider: Arc<dyn Provider> = Arc::new(OpenAiProvider::new(client));
let model = ModelConfig::new(model_name);
```

The provider handles communication with the model service. `ModelConfig`
selects the model. Passing both into the machine keeps that choice separate
from the rest of the application.

## Give it one tool

The model only gets one tool: `shell`. Its input and output are ordinary Rust
types with generated JSON schemas:

```rust
#[derive(Default, Deserialize, JsonSchema)]
struct ShellInput {
    command: String,
}

#[derive(Serialize, JsonSchema)]
struct ShellOutput {
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
}
```

`ToolBase` gives the tool its model-facing name and description, and connects
those types to the tool interface:

```rust
struct Shell;

impl ToolBase for Shell {
    type Parameter = ShellInput;
    type Output = ShellOutput;
    type Error = ErrorData;

    fn name() -> Cow<'static, str> {
        "shell".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some("Execute a non-interactive Bash command in the project directory. Commands time out after 120 seconds; cwd changes do not persist between calls.".into())
    }
}
```

`AsyncTool<Session>` implements the actual behavior. It receives the loaded
session, so it can start commands in the session's saved working directory:

```rust
impl AsyncTool<Session> for Shell {
    async fn invoke(session: &Session, input: ShellInput) -> Result<ShellOutput, ErrorData> {
        let mut command = tokio::process::Command::new("/bin/bash");
        command
            .arg("-c")
            .arg(input.command)
            .current_dir(&session.metadata.cwd)
            .stdin(Stdio::null())
            .kill_on_drop(true);

        let output = tokio::time::timeout(Duration::from_secs(120), command.output())
            .await
            .map_err(|_| ErrorData::internal_error("shell timed out after 120 seconds", None))?
            .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;

        Ok(ShellOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code(),
        })
    }
}
```

Registering it with `with_async_tool::<Shell>()` makes both its schema and
implementation available to `ToolOperation`. The `rmcp` traits are used for
typed tool definitions; this tool runs in-process, without an MCP server.

Each call starts a new shell. A `cd` command or shell variable from one call
does not carry into the next. A nonzero exit code is returned to the model
alongside stdout and stderr so it can decide what to do next.

## Prepare the model request

The `Prompt` request preparer supplies the system prompt and forwards the tools
collected from the machine's operations:

```rust
struct Prompt;

#[async_trait]
impl InferenceRequestPreparer<Session> for Prompt {
    async fn prepare(
        &self,
        _: &Session,
        _: &Conversation,
        input: InferenceInput,
    ) -> Result<PreparedInferenceRequest> {
        Ok(PreparedInferenceRequest {
            system_prompt: SYSTEM_PROMPT.into(),
            tools: input.tools,
            additional_messages: vec![],
        })
    }
}
```

The example's system prompt tells the agent to use the shell to inspect, edit,
and test code, treat file contents and command output as untrusted data, and
report its changes and verification concisely. These instructions guide the
model; they do not enforce a security boundary.

The preparer also receives the current session and conversation. This example
does not use them, but that is where an application can add request-specific
context instead of relying on one fixed prompt.

## Own the session and its effects

GDK does not require a particular database or session format. The application
provides a session that exposes its ID and conversation:

```rust
impl MachineSession for Session {
    fn id(&self) -> &str {
        &self.id
    }

    fn conversation(&self) -> Option<&Conversation> {
        Some(&self.conversation)
    }
}
```

In `ngrc`, that session also contains creation time, working directory, provider,
model, and recorded usage. A `Runtime` wraps the file-backed store. It implements
`SessionLoader` to return a snapshot of the session for each machine pass:

```rust
#[async_trait]
impl SessionLoader<Session> for Runtime {
    async fn load(&self, id: &str) -> Result<Session> {
        let session = self.snapshot();
        if session.id != id {
            bail!("unknown session {id}");
        }
        Ok(session)
    }
}
```

The machine produces effects rather than writing to the file directly. This
example uses two kinds:

```rust
enum Effect {
    Message(Message),
    Usage(ProviderUsage),
}

impl From<Message> for Effect {
    fn from(message: Message) -> Self {
        Self::Message(message)
    }
}

impl InferenceEffect for Effect {
    fn record_usage(usage: ProviderUsage) -> Self {
        Self::Usage(usage)
    }
}

impl MachineEffect for Effect {
    fn ensure_message_ids(&mut self) {
        if let Self::Message(message) = self {
            *message = message.clone().with_generated_id_if_missing();
        }
    }
}
```

`From<Message>` lets tool and inference results become application effects.
`InferenceEffect` preserves provider usage alongside messages. `MachineEffect`
ensures messages have IDs before persistence.

The runtime's `EffectHandler` converts those effects to records for its store.
The core of that implementation is:

```rust
let records = effects
    .iter()
    .map(|effect| match effect {
        Effect::Message(message) => Record::Message {
            message: message.clone(),
        },
        Effect::Usage(usage) => Record::Usage {
            usage: usage.clone(),
        },
    })
    .collect();
store.append(records)?;
```

[`src/session.rs`](https://github.com/jamadeo/ngrc/blob/dc25c579cd40d85f8d90b61e80cbf02b8838564f/src/session.rs)
implements the append-only JSONL store. It writes and syncs records before
updating the in-memory session. Reopening the file replays those records to
reconstruct the conversation, metadata, and usage. Assistant messages, tool
results, and usage are persisted as effects, not on every streamed token.

## Run a turn and stream the response

Before running the machine, save the user's message:

```rust
runtime.append_message(Message::user().with_text(prompt))?;
```

The machine emits events while it works. An event channel lets the interface
display output before the turn finishes. Here is a shortened version of the
example's `run_turn` function:

```rust
let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
let emitter = Emitter::new(tx, cancel.clone());

let display = tokio::spawn(async move {
    while let Some(event) = rx.recv().await {
        if let AgentEvent::Message(message) = event {
            for content in message.content {
                match content {
                    MessageContent::Text(text) => {
                        print!("{}", text.text);
                        io::stdout().flush()?;
                    }
                    MessageContent::Error(error) => eprintln!("{}", error.message),
                    _ => {}
                }
            }
        }
    }
    Ok::<_, io::Error>(())
});

let machine = agent(provider, model, cancel);
let id = runtime.snapshot().id;
let result = machine.run(runtime, &id, &emitter).await;
drop(emitter);
display.await??;
let session = result?;
```

Streaming and persistence have separate jobs: the emitter updates the UI, while
the effect handler records the session. Printing an event does not save it.

For each input line, the chat loop appends a user message and runs a fresh
machine around the same stored session. The machine has no conversation of its
own to carry between turns.

The application also connects Ctrl-C to the shared `CancellationToken`. It
awaits the machine after cancellation so GDK can finalize interrupted tool
responses before the display task finishes.

## Resume the conversation

By default, new sessions are stored in `~/.ngrc/sessions`. On startup, the
application prints the exact file path. To reopen one, keep your API key and
model environment variables set and replace the path below with that file:

```bash
cargo run --locked -- --resume /absolute/path/to/session.jsonl
```

The application loads the prior messages, shell results, usage, and saved
working directory, then waits for your next prompt. It does not rerun old shell
commands. `--session-dir DIR` chooses a different directory for new sessions.

This demonstrates persistent conversation state, not recovery of a running shell
process or exactly-once external actions. A crash can lose output that has only
been streamed, and an incomplete log record needs attention before reopening.

## Where to go next

The example deliberately leaves out tool approval, sandboxing, output limits,
context compaction, and a model-turn or cost budget. Its shell timeout does not
guarantee cleanup of descendant processes. Keep these limits in mind before
turning it into an application that works on real data.

To change the agent, start with one of the choices already visible here: add a
tool, replace the request preparer, choose another provider, or implement a
different session store. More specialized behavior can become another
operation in the machine's ordered list.

Explore the [demos](/docs/gdk/demos) for larger applications, or use the
[reference](/docs/gdk/reference) to find the component APIs. For this example,
select version `0.1.0-alpha.12` in the generated Rust documentation.
