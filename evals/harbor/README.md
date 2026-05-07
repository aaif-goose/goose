# Harbor

This directory contains a developer tool for running Harbor benchmark datasets
with Goose.

The runner takes a prebuilt Goose executable, writes a Harbor job config, and
runs Harbor with the local `goose_harbor` adapter.

## Requirements

- `uv`
- `harbor`
- Docker, for Docker-backed Harbor datasets
- A Goose executable compatible with the benchmark task environment

Dependencies are declared in `pyproject.toml`. `uv` resolves them from the
developer's configured package index.

## Run A Task

```bash
uv run --project evals/harbor evals/harbor/run \
  --goose-binary ./target/x86_64-unknown-linux-gnu/release/goose \
  --goose-profile ~/.config/goose-benchmark \
  --dataset terminal-bench/terminal-bench-2 \
  --model databricks/<model-name> \
  --task terminal-bench/fix-git \
  --trials 1 \
  --concurrency 1
```

Use `--dry-run` to write the Harbor config without starting the benchmark:

```bash
uv run --project evals/harbor evals/harbor/run \
  --goose-binary ./target/x86_64-unknown-linux-gnu/release/goose \
  --goose-profile ~/.config/goose-benchmark \
  --dataset terminal-bench/terminal-bench-2 \
  --model databricks/<model-name> \
  --task terminal-bench/fix-git \
  --dry-run
```

Outputs default to:

```text
evals/harbor/.runs/configs/
evals/harbor/.runs/jobs/
```

Override them with `--config-dir` and `--jobs-dir`.

## Goose Executable

`--goose-binary` must point to a Goose executable that can run inside the
benchmark task container. The runner does not build Goose for you; it uploads
the executable you provide into each task container and runs that copy.

For Terminal-Bench 2.0, use a Linux amd64 Goose binary.

On Linux:

```bash
cargo build --release -p goose-cli --bin goose
uv run --project evals/harbor evals/harbor/run --goose-binary ./target/release/goose ...
```

On macOS or Windows, use a cross-compiled Linux amd64 binary. Prefer a binary
built for benchmark/container use. In particular, a Goose CLI binary without
local inference is usually the best fit for Harbor runs because local inference
pulls in runtime dependencies that may not exist in benchmark task images.

When using a GitHub release binary for Terminal-Bench, use the standard Linux
amd64 artifact, not the Vulkan artifact.

Some Linux release binaries still require GCC's OpenMP runtime, packaged as
`libgomp1` on Debian and Ubuntu. If the binary fails to start with a missing
`libgomp.so.1` error, rerun with:

```bash
uv run --project evals/harbor evals/harbor/run \
  --goose-binary ./goose \
  --goose-profile ~/.config/goose-benchmark \
  --dataset terminal-bench/terminal-bench-2 \
  --model databricks/<model-name> \
  --install-goose-runtime-deps
```

This installs only the minimal known Goose runtime dependency, currently
`libgomp1`, inside each Debian/Ubuntu task container before Goose starts. Leave
it off when the provided Goose executable can start in the task container
without extra OS packages.

For local models, prefer running Ollama or llama.cpp outside the task container
and configuring Goose to call that server through its normal provider/profile
configuration. Avoid running local inference inside each benchmark task
container unless you have specifically built and verified a compatible Goose
binary for that environment.

## Goose Profile

Pass `--goose-profile` to copy an explicit Goose profile into each benchmark
task container. The path can be either:

- a `GOOSE_PATH_ROOT` directory with `config/`, `data/`, and `state/`
- a Goose config directory containing `config.yaml`

The adapter sets `GOOSE_PATH_ROOT` inside the container after copying the
profile. `--model provider/model` still selects the provider and model for the
benchmark run.

If the profile contains `secrets.yaml`, that file will be copied into arbitrary
benchmark task containers. Prefer benchmark-scoped or disposable credentials.

## Local Models

For local models, prefer running the model server on the host and configuring
the benchmark profile to reach it from the task container. This keeps model
loading and hardware acceleration outside Docker while Goose runs inside the
benchmark environment.

For example, an Ollama profile can set:

```yaml
GOOSE_PROVIDER: ollama
GOOSE_MODEL: qwen3.6:27b
OLLAMA_HOST: http://host.docker.internal:11434
```

Then run with `--goose-profile` pointing at that profile and `--model
ollama/qwen3.6:27b`.

Running Goose's built-in local inference inside the benchmark container is less
portable: the model file, CPU/GPU support, target architecture, and container
runtime all have to line up.

## Tests

```bash
uv run --project evals/harbor pytest evals/harbor/tests
```
