---
title: Providers
description: Choose a model backend, use declarative definitions, or implement the provider contract yourself.
displayed_sidebar: gdkSidebar
---

import ProvidersDiagram from "@site/static/img/gdk/providers.svg";

# Providers

A provider connects your application to a model backend. It translates GDK
messages and tool definitions into the backend's request format, and translates
the response back into shared message and usage types. The rest of your agent
does not need to speak each service's wire protocol.

The provider and model are separate choices. A provider knows how to reach a
service; `ModelConfig` selects a model and request settings. You can use a
provider directly for a completion, or pass it to an `InferenceRunner` in a
[state machine](/docs/gdk/concepts/state-machine).

<ProvidersDiagram />

## The provider contract

The `Provider` trait lives in `goose-provider-types`. Its central method is
`stream`: given a model configuration, system prompt, conversation messages,
and tool schemas, it returns a `MessageStream`. Text can arrive in small chunks;
tool calls are emitted once their arguments have been assembled. The stream can
also carry usage information and errors.

The default `complete` method collects the stream into one message and usage
record. Providers can also report model inventories, context limits, reasoning
capabilities, and whether they manage their own context. A common interface
does not mean every model supports the same content or tools: choose a model
that supports the capabilities your application needs.

For example, inside an async Rust function returning `anyhow::Result`, a direct
completion can look like this. Enable a TLS feature on `goose-providers` for
the HTTPS connection:

```rust
use goose_provider_types::{
    base::Provider,
    conversation::message::Message,
    model::ModelConfig,
};
use goose_providers::{
    api_client::{ApiClient, AuthMethod},
    openai::OpenAiProvider,
};

let client = ApiClient::new_with_tls(
    "https://api.openai.com".into(),
    AuthMethod::BearerToken(std::env::var("OPENAI_API_KEY")?),
    None,
)?;
let provider = OpenAiProvider::new(client);
let model = ModelConfig::new(std::env::var("OPENAI_MODEL")?);
let messages = [Message::user().with_text("Say hello in one sentence.")];

let (reply, usage) = provider
    .complete(&model, "Answer concisely.", &messages, &[])
    .await?;
```

This is a model call, not an agent run. Supplying tool schemas lets a model
request tools; it does not execute them. Tool execution belongs to your
application or an [operation](/docs/gdk/concepts/operations).

## Implementations that ship with GDK

`goose-providers` includes implementations for OpenAI, Anthropic, Google Gemini,
Azure AI Foundry, Databricks, Snowflake, OpenRouter, Ollama, and OpenAI-compatible
endpoints. It re-exports the shared provider types, so applications using these
implementations can import their common types from the same crate.

Many additional services use bundled declarative definitions, including Groq,
Mistral, Together, Cerebras, DeepSeek, and LM Studio. The crate also has separate
interfaces for live sessions and voice, and for typed decision requests. Those
serve different interaction patterns; they are not all interchangeable calls
to the text-completion `Provider` trait.

The [provider reference](https://docs.rs/goose-providers/latest/goose_providers/)
lists the available modules and feature flags. Credentials and HTTP authentication
are covered in [Authentication](/docs/gdk/concepts/authentication).

## Declarative providers

If a service already speaks a supported protocol, an endpoint definition may be
enough. A declarative provider selects an OpenAI, Anthropic, or Ollama engine,
then supplies the endpoint, configuration keys, model metadata, and optional
headers or behavior flags. This avoids writing a Rust implementation for each
service that shares the same protocol.

Definitions can be bundled with your application or loaded at runtime. For
example, this definition describes an application-controlled OpenAI-compatible
endpoint. Replace the example URL and model metadata with the real values:

```json
{
  "name": "my-service",
  "display_name": "My model service",
  "engine": "openai",
  "base_url": "https://models.example/v1/chat/completions",
  "api_key_env": "MY_SERVICE_API_KEY",
  "models": [
    {
      "name": "my-model",
      "context_limit": 32768
    }
  ],
  "dynamic_models": false
}
```

Create the provider with `declarative::from_json`, or inspect definitions with
`deserialize_provider_config`. `EnvKeyResolver` reads credentials from the
environment; your application can supply another `KeyResolver` for its own
credential store. `load_custom_providers` reads definitions from a directory.
Treat definitions as trusted configuration: they choose where requests and
credentials go, and can include a credential-fetching command.

## Local models and providers

There are two ways to use models running on the same machine:

| Approach                   | What your application connects to                                                                                                                                   |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Local model server         | An Ollama or OpenAI-compatible HTTP endpoint, such as one exposed by LM Studio. The server owns model loading and inference.                                        |
| In-process local inference | `LocalInferenceProvider` from `goose-local-inference`, available through the `local-inference` feature on `goose-providers`. Your process loads and runs the model. |

In-process inference uses llama.cpp for GGUF models, with optional MLX support
on Apple silicon. Models can be selected by a local file path; optional Hugging
Face support adds discovery, downloads, and a cached-model inventory. CUDA,
Vulkan, and MLX features select the relevant backends and have native build
requirements.

With local inference enabled, the provider still implements the same contract:

```rust
use goose_providers::local_inference::LocalInferenceProvider;

let provider = LocalInferenceProvider::from_env().await?;
let model = ModelConfig::new("/absolute/path/to/model.gguf");
```

You can call `stream` or `complete`, or use it for agent inference as above.
Local placement does not guarantee a model fits in memory or supports reliable
tool calling. The local-inference crate handles chat templates, parsing, model
residency, and eviction; your application still chooses the model and when to
use it. [lifewiki](/docs/gdk/demos/lifewiki) is an example with an offline path.

See the [local-inference reference](/docs/gdk/reference#goose-local-inference)
for the engine, features, and model-management APIs.

## Implementing your own provider

Write a provider when a backend's protocol or behavior cannot be represented
by an existing implementation or declarative definition. Implement `get_name`
and `stream`; `complete` and several discovery methods have defaults. Depend on
`goose-provider-types` if you only need that contract, and use the request and
formatting helpers from `goose-providers` where they fit.

Normalize the backend's output into GDK messages, assemble tool calls before
emitting them, preserve usage information, and map failures into `ProviderError`.
Report capabilities and context limits honestly rather than assuming that a
successful text response implies support for tools, images, or reasoning.
Provider-specific authentication can remain separate through `ApiClient` and
`AuthProvider`.

The full trait is in
[`goose-provider-types::base`](https://docs.rs/goose-provider-types/latest/goose_provider_types/base/).
For an existing implementation to read, start with the
[provider source](https://github.com/aaif-goose/goose/tree/main/crates/goose-providers/src).
