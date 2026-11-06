# goose-local-inference

On-device model inference for Goose applications, available through
[`goose-providers`](../goose-providers) as a standard provider. The crate manages
model loading, streaming responses, cancellation, and tool calling, with image
input and reasoning output where supported by the model and backend.

## Backends and models

llama.cpp runs GGUF models across platforms. Eredu provides MLX inference on Apple
silicon for SafeTensors and GGUF models. Model compatibility depends on the
backend, checkpoint, and chat template; a model supported by one engine may not
work with the other.

Models can be loaded from local paths or discovered and downloaded through
Hugging Face. Local files remain user-owned, while downloaded models are managed
through the Hugging Face cache. Gated repositories require Hugging Face access.

Automatic backend selection uses llama.cpp for GGUF and Eredu for SafeTensors.
Applications can select a backend explicitly. Native tool calling is used when
supported, with tool emulation available for other models.

## Integration

Use this crate when you want inference and model lifecycle management inside your
application. It requires a native build toolchain and enough memory for the model
and its working context. Hardware acceleration depends on the platform and build;
see [Cargo.toml](Cargo.toml) for available features and dependencies.

Start with the provider integration in [`goose-providers`](../goose-providers).
Model size alone does not determine memory use: context length, image input, and
inference settings also matter.
