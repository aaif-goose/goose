# Lean Goose CLI

## Summary

The Goose CLI does **not need functionality cuts to reach a 50 MB binary**. The current binary is primarily large because the workspace has no size-oriented release profile.

The baseline build:

```bash
cargo build -p goose-cli --bin goose \
  --no-default-features --release --features native-tls
```

produces a binary that is approximately 111 MiB on disk (116.4 MiB according to `cargo bloat`). Building the exact same feature set with size-oriented release settings produces a working binary of approximately **32 MiB**, a reduction of roughly **72%**.

## Measurements

### Baseline

Command:

```bash
cargo build -p goose-cli --bin goose \
  --no-default-features --release --features native-tls
```

Results on macOS ARM64:

- File size from `cargo bloat`: 116.4 MiB
- File size from `du`: approximately 111 MiB
- `.text`: 60.2 MiB
- Stripping the completed binary afterward: approximately 95 MiB

Stripping alone is therefore not enough to reach the target.

### Size-oriented build

Command:

```bash
CARGO_TARGET_DIR=target/lean \
CARGO_PROFILE_RELEASE_STRIP=symbols \
CARGO_PROFILE_RELEASE_LTO=fat \
CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
CARGO_PROFILE_RELEASE_OPT_LEVEL=z \
CARGO_PROFILE_RELEASE_PANIC=abort \
cargo build -p goose-cli --bin goose \
  --no-default-features --release --features native-tls
```

Result:

```text
target/lean/release/goose: 32 MiB
```

The resulting binary runs successfully:

```text
goose 1.49.0
```

### Mach-O comparison

| Section | Existing release | Size-oriented release |
|---|---:|---:|
| File | 116.4 MiB | 32 MiB |
| `__text` | 60.2 MiB | 13.5 MiB |
| `__const` | 16.4 MiB | 15.1 MiB |
| Exception tables | 4.6 MiB | 7 KiB |
| Unwind info | 1.3 MiB | 167 KiB |
| `__eh_frame` | 7.3 MiB | 79 KiB |
| Link-edit/debug/symbol data | 18.1 MiB | 480 KiB |

## Why the size-oriented build works

### Fat LTO and one codegen unit

Cross-crate link-time optimization allows dead-code elimination and deduplication across crate boundaries. A single codegen unit gives LLVM the broadest view of the program, at the cost of longer builds.

### `panic = "abort"`

Aborting on panic removes most of the unwind machinery. The large reduction in exception tables and `__eh_frame` demonstrates the impact.

Before adopting this setting, verify that no supported CLI behavior depends on catching panics.

### `opt-level = "z"`

This asks LLVM to optimize generated code for minimum size rather than execution speed. It is appropriate for a distribution profile, though performance-sensitive workloads should be benchmarked.

### `strip = "symbols"`

This removes symbol and link metadata from the distributed binary. On its own it reduces the baseline only to approximately 95 MiB, but it is effective in combination with LTO and size optimization.

## Recommended implementation

Add a named Cargo profile so ordinary release build performance and compilation times remain unchanged:

```toml
[profile.lean]
inherits = "release"
opt-level = "z"
lto = "fat"
codegen-units = 1
panic = "abort"
strip = "symbols"
```

Build it with:

```bash
cargo build -p goose-cli --bin goose \
  --no-default-features \
  --profile lean \
  --features native-tls
```

A `just lean-binary` recipe or equivalent release job would make the build reproducible.

It would also be useful to add a CI size check with a threshold around 40–45 MiB, leaving headroom below the 50 MB target while catching regressions.

## `cargo bloat` findings

The baseline `.text` contribution by crate includes:

| Crate | `.text` contribution |
|---|---:|
| `goose` | 18.9 MiB |
| `std` | 9.9 MiB |
| `rmcp` | 3.2 MiB |
| `goose-mcp` | 2.4 MiB |
| `goose-cli` | 1.7 MiB |
| `goose-provider-types` | 1.4 MiB |
| `docx-rs` | 1.2 MiB |
| `umya-spreadsheet` | 1.0 MiB |
| `image` | 900 KiB |
| ACP | 878 KiB |
| `goose-providers` | 866 KiB |
| `jsonschema` | 754 KiB |
| `reqwest` | 704 KiB |
| `minijinja` | 691 KiB |
| `aws-lc-sys` | 686 KiB |

These cuts are not required to meet the 50 MB goal, but they are opportunities to make the dependency graph and product definition genuinely lean.

## Further dependency and code-cut opportunities

### 1. Feature-gate rich document support

`goose-mcp` unconditionally includes:

- `docx-rs`
- `umya-spreadsheet`
- `lopdf`
- `image` with a broad set of image formats

Together these account for several MiB of generated code and pull in duplicate image, TIFF, GIF, PNG, regex, ZIP, and crypto implementations.

A feature could isolate these tools:

```toml
computer-controller-documents = [
    "dep:docx-rs",
    "dep:umya-spreadsheet",
    "dep:lopdf",
    "dep:image",
]
```

A lean CLI could omit DOCX, XLSX, and PDF tooling while retaining ordinary MCP functionality.

### 2. Feature-gate the autovisualiser

`goose-mcp` embeds approximately 4 MiB of frontend assets:

- `mermaid.min.js`: 3.2 MiB
- D3: 276 KiB
- Chart.js: 204 KiB
- Leaflet and supporting assets

This is why constant data remains around 15 MiB even after aggressive LTO. An `autovisualiser` feature would provide an immediate and predictable file-size reduction.

### 3. Remove Rustls from native-TLS builds

The `native-tls` build unexpectedly still compiles:

- `rustls`
- `aws-lc-rs`
- `aws-lc-sys`
- `tokio-rustls`
- `hyper-rustls`
- `rustls-platform-verifier`

The dependency graph contains both `reqwest` 0.13 and a `reqwest` 0.12 path through `oauth2`. The selected `reqwest` features retain Rustls even though the top-level build requests native TLS.

Correcting feature propagation and consolidating `reqwest` versions should remove approximately 1.2 MiB directly visible in `.text`, reduce duplicated generic HTTP code, improve build time, and eliminate unnecessary crypto dependencies.

### 4. Feature-gate ACP server and gateway functionality

Even the no-default-features CLI contains ACP HTTP server and dispatch code. Several of the largest individual symbols reported by `cargo bloat` are ACP dispatch handlers.

If the lean CLI only needs interactive and one-shot agent sessions, consider gating:

- ACP server support
- ACP HTTP support
- Gateway/server commands
- Related Axum server support

### 5. Feature-gate secondary CLI commands

The no-default-features build still contains functionality for:

- Scheduling and SQLite
- Recipe tooling
- Review orchestration
- Shell completion generation
- Rich terminal syntax rendering through `bat`, `syntect`, and Oniguruma
- Gateway/server support

These are smaller individually, but a dedicated lean product should explicitly define its supported command set rather than linking every command.

## Proposed priority

1. Add the named `lean` profile. This has a measured result of 32 MiB with no feature cuts.
2. Add a CI binary-size regression check.
3. Feature-gate autovisualiser assets.
4. Feature-gate document tooling in `goose-mcp`.
5. Correct native-TLS feature propagation so Rustls and AWS-LC are absent.
6. Only then consider cutting ACP, scheduling, or other user-facing CLI commands.

## Proposed feature boundaries

The full Goose build should remain behaviorally unchanged by keeping a `full` aggregate in the default feature set. The lean binary should explicitly select a small `lean` aggregate. Features should represent coherent product capabilities rather than individual dependencies.

### Feature policy

```toml
[features]
default = ["full"]
full = [
    "lean",
    "acp-web",
    "acp-management",
    "bundled-mcp",
    "scheduler",
    "session-search",
    "extended-providers",
    "rich-cli",
    "rich-terminal",
    "advanced-agent",
]
lean = [
    "acp-core",
    "developer-tools",
    "session-storage",
    "openai-provider",
    "native-tls",
]
```

This is illustrative: Cargo features are additive, so the implementation must ensure `lean` does not accidentally activate `full` through a dependency's defaults. Existing public features such as `code-mode`, `tree-sitter`, `local-inference`, telemetry, roaming, and TLS selection remain independent.

### `goose-mcp` features

`goose-mcp` should stop compiling every server unconditionally.

| Feature | Modules/dependencies | Lean |
|---|---|---:|
| `autovisualiser` | `autovisualiser`, embedded JS/CSS assets | No |
| `computer-controller` | Base computer controller/Peekaboo integration | No |
| `document-tools` | DOCX, XLSX, PDF modules; `docx-rs`, `umya-spreadsheet`, `lopdf`, `image` | No |
| `memory-server` | `memory` | No |
| `tutorial-server` | `tutorial`, `include_dir` tutorial content | No |
| `bundled-mcp` | Aggregate of all preceding server features | No |

`goose-mcp` should have no default server features at its library layer. The full CLI enables `bundled-mcp`; lean does not depend on `goose-mcp`. The developer extension is unaffected because it is implemented inside `goose`, not `goose-mcp`.

### ACP features in `goose`

ACP needs a core/management split rather than a single on/off gate.

| Feature | Includes | Lean |
|---|---|---:|
| `acp-core` | ACP stdio transport, initialize, new/load session, prompt/cancel, streaming updates, permissions, tool notifications, core filesystem methods | Yes |
| `acp-recipes` | Recipe custom requests and conversion APIs | No |
| `acp-provider-management` | Provider inventory/configuration/auth custom requests | No |
| `acp-apps` | App cache, app APIs, MCP-app proxy | No |
| `acp-web` | HTTP/WebSocket server, Axum, CORS, TLS certificate hosting | No |
| `acp-management` | Aggregate of recipes, provider management, and apps | No |
| `acp-full` | `acp-core`, `acp-management`, and `acp-web` | Full only |

`agent-client-protocol` and its schema remain in core. `agent-client-protocol-http`, Axum server-only features, `axum-server`, `rcgen`, and server TLS dependencies move behind `acp-web`. Native TLS for outbound provider traffic must not imply ACP web hosting.

### Platform-extension features in `goose`

Each platform extension currently registered unconditionally should be independently removable.

| Feature | Extension/module | Lean |
|---|---|---:|
| `developer-tools` | `platform_extensions::developer`, ACP filesystem support | **Yes, mandatory** |
| `todo-tools` | `todo` | No |
| `apps-tools` | `apps`, app cache/templates | No |
| `chat-recall` | `chatrecall`, session search | No |
| `extension-manager-tools` | `ext_manager` | No |
| `scheduler-tools` | scheduler extension | No |
| `subagent-tools` | `summon`, orchestrator, subagent execution | No |
| `summarize-tools` | `summarize` | No |
| `tom-tools` | `tom` | No |
| `code-mode` | Existing code execution feature | No |
| `tree-sitter` | Existing analyze feature and grammars | No |
| `platform-tools-full` | Aggregate of all platform extensions | Full only |

Both the legacy loop and state-machine path must compile and behave correctly with only `developer-tools`, per the ongoing agent-loop parity requirement.

### Session and scheduler features in `goose`

| Feature | Includes | Lean |
|---|---|---:|
| `session-storage` | Core session CRUD, messages, metadata, ACP load/resume/fork, extension state | Yes |
| `session-search` | Chat history full-text/filter search and snippets | Initially no |
| `session-import-export` | Import formats and Markdown export | No |
| `session-naming` | LLM-generated names and related prompt path | Optional after measurement |
| `nostr` | Existing Nostr sharing | No |
| `scheduler` | Scheduler, scheduler trait, cron dependency, scheduler extension and CLI | No |

The first implementation should leave SQLx/SQLite inside `session-storage`; separating scheduler and search first avoids coupling the storage decision to the rest of the work. Once the lean build is measurable, introduce a `SessionStore` boundary and compare SQLx SQLite with a narrower SQLite implementation. Avoid exposing backend selection as user-facing Cargo features until there are two working backends.

### Provider features

The broad provider modules need compile-time boundaries in both `goose-providers` and the compatibility provider modules still in `goose`.

| Feature | Providers/dependencies | Lean |
|---|---|---:|
| `openai-provider` | One OpenAI-compatible HTTP implementation and streaming formats | Yes |
| `anthropic-provider` | Anthropic implementation | Later, measured |
| `oauth-providers` | OAuth callback server and OAuth2 | No |
| `gcp-providers` | Vertex/GCP auth, JWT and asymmetric crypto | No |
| `enterprise-providers` | Azure, Databricks, Snowflake | No |
| `local-cli-providers` | Claude Code, Codex, Gemini CLI, Cursor-style subprocess providers | No |
| `local-http-providers` | Ollama and local discovery | No |
| `declarative-providers` | Embedded provider definitions and dynamic declarative layer | No |
| `model-catalog` | Embedded 4.2 MiB canonical model catalog | No |
| `extended-providers` | Aggregate of all provider groups | Full only |

Lean accepts arbitrary model strings and should not require the catalog. Shared provider types must also be split so selecting `openai-provider` does not compile every wire format, model registry, image/document helper, and RMCP server macro.

### Agent capability features

| Feature | Modules/dependencies | Lean |
|---|---|---:|
| `external-mcp` | MCP client and stdio child transport | Add after measuring core |
| `mcp-http` | Streamable HTTP/SSE transports and auth | No initially |
| `dictation` | Hosted transcription path | No |
| `local-inference` | Existing Candle/audio stack | No |
| `images` | Image decoding and attachment transformations | No initially |
| `recipes` | Recipe parsing/rendering/deeplinks | No |
| `skills` | Built-in skills and embedded content | No |
| `hooks` | Hook execution and plugin hook configuration | No |
| `plugins` | Plugin discovery/loading | No |
| `context-compaction` | Token counting, Tiktoken, context management | Reassess after baseline |
| `security-scanners` | Malware/adversary/prompt-injection scanners | Reassess; preserve essential developer-tool safety |
| `advanced-agent` | Aggregate of all non-core agent facilities | Full only |

Developer command approval and dangerous-command protections are not optional merely because richer scanners are gated. The developer feature must define and test the minimum required safety behavior explicitly.

### CLI features

| Feature | Commands/dependencies | Lean |
|---|---|---:|
| `cli-session` | Basic interactive session and run command | Yes |
| `cli-acp` | ACP stdio command | Yes |
| `cli-configure` | Interactive configuration UI | No initially |
| `cli-doctor` | Doctor command | No |
| `cli-recipes` | Recipe commands | No |
| `cli-review` | Review orchestration | No |
| `cli-plugins` | Plugin commands | No |
| `cli-scheduler` | Schedule commands | No |
| `cli-skills` | Skills commands | No |
| `cli-terminal` | Terminal integration | No |
| `cli-gateway` | Gateway commands | No |
| `cli-completions` | Shell completion/manpage generation | No |
| `update` | Existing updater/signature dependencies | No |
| `rich-terminal` | Bat/Syntect/Oniguruma, Cliclack, tables, progress presentation | No |
| `rich-cli` | Aggregate of secondary commands | Full only |

The lean CLI should use plain streaming output. Rich formatting should be isolated from session construction so disabling it does not alter agent semantics.

### Other existing opt-in features excluded from lean

The current `telemetry`, `otel`, `nostr`, `system-keyring`, `roaming`, `aws-providers`, `local-inference`, `code-mode`, and `tree-sitter` features remain disabled. TLS remains an explicit mutually exclusive choice. Update support remains disabled.

### Implementation order

Feature gating should proceed in dependency-sized slices, measuring the lean binary after every slice:

1. Add the named `lean` profile and a size-report script; record the 32 MiB baseline.
2. Split `goose-mcp`; remove it entirely from the lean dependency graph.
3. Gate scheduler modules, platform tool, CLI command, and cron dependency together.
4. Gate embedded model catalog and declarative/extended providers.
5. Split platform extensions, retaining only developer in lean.
6. Split ACP core from web and management APIs while maintaining ACP tests.
7. Split CLI commands and rich terminal rendering.
8. Split session search/import/export from core storage; retain SQLx initially.
9. Split advanced agent facilities and external MCP transports.
10. If still above 10 MiB, extract a minimal agent/provider/type core and replace SQLx only after a measured backend comparison.

Each slice must preserve the default full build. Avoid one giant `lean` conditional throughout the code; gate modules with capability features and make `lean` only an aggregate.

### Verification matrix

Every feature slice should run:

- `cargo fmt`
- `cargo check -p goose --no-default-features --features "acp-core,developer-tools,session-storage,openai-provider,native-tls"`
- `cargo check -p goose-cli --no-default-features --features lean`
- Existing full/default crate tests for touched modules
- ACP core integration tests in the lean configuration
- Developer read/write/edit/shell tests in the lean configuration
- Session create/save/load/resume/fork tests in the lean configuration
- Both legacy agent-loop and `GOOSE_STATE_MACHINE=1` tests where agent behavior changes
- Release size measurement and dependency-tree assertion that excluded crates are absent
- Full workspace Clippy before merging the completed series

Useful dependency assertions include absence of `goose-mcp`, document crates, image codecs, `tokio-cron-scheduler`, AWS SDKs, OpenTelemetry, Bat/Syntect/Oniguruma, `agent-client-protocol-http`, Axum server dependencies, and Rustls/AWS-LC from the native-TLS lean graph.

## Reaching approximately 10 MB

A 10 MB target is qualitatively different from the 50 MB target. Compiler settings alone reduce the current no-default-features CLI to 32 MiB, but getting close to 10 MB requires defining a separate, substantially smaller product rather than continuing to trim the existing all-purpose binary.

### Measured lower bound

A prototype macOS ARM64 CLI using only Tokio, Reqwest with native TLS, Serde, and JSON was built with the same size-oriented profile. It performs a real OpenAI chat-completions request in an interactive input loop.

Its stripped binary was approximately **1 MiB** (`ls`) to **1.8 MiB** (`cargo bloat`), with a 668 KiB `.text` section. This demonstrates that HTTP, native TLS, async I/O, and JSON are not barriers to a 10 MB binary. The remaining size in Goose comes from product surface and generic instantiations, not the minimum viable network stack.

### Current 32 MiB composition

The size-oriented Goose binary contains approximately:

- 13.5 MiB of executable code
- 15.1 MiB of constant data
- 3.4 MiB of remaining Mach-O sections and metadata

Two especially large embedded data sets are visible in the source tree and resulting binary:

- Canonical model catalog: 4.2 MiB
- MCP autovisualiser assets: approximately 4 MiB

Removing or externalizing those gets the binary only into the low-to-mid 20 MiB range. Reaching 10 MB therefore also requires removing a majority of the linked code.

### Required product cuts

A realistic 10 MB Goose should retain only:

- One-shot `run` and, optionally, a basic interactive loop
- ACP operation as a first-class requirement
- The in-process developer extension
- One or a very small number of HTTP providers
- Streaming text responses
- Persistent session storage
- External MCP client support only if it fits after measurement
- A minimal terminal renderer

It should remove or move to separately installed components:

1. **All optional bundled MCP servers**
   - Remove `goose-mcp` from the lean executable.
   - This removes the autovisualiser, computer controller, DOCX, XLSX, PDF, image codecs, memory server, tutorials, and their RMCP server instantiations.
   - The developer extension is not in `goose-mcp`; it is an in-process platform extension under `goose::agents::platform_extensions::developer`, so it can and should remain.
   - External MCP servers can still be launched over stdio if MCP client support is retained.

2. **ACP must remain, but narrow its surface**
   - Keep the ACP protocol, stdio transport, session lifecycle, prompt/response updates, tool calls, permission requests, and filesystem integration used by developer tools.
   - Feature-gate ACP HTTP/WebSocket serving, Axum server support, CORS/TLS hosting, MCP-app proxying, provider administration, recipe administration, gateway integrations, and desktop-only custom requests.
   - ACP is currently heavily coupled to provider inventory, recipes, apps, and configuration management. A lean ACP handler should implement the standard core protocol without linking every desktop management API.

3. **The universal provider registry**
   - Do not link every provider implementation into the lean executable.
   - Start with one OpenAI-compatible provider. Anthropic could be a second measured feature.
   - Remove Vertex/GCP auth, OAuth flows, JWT/RSA/ECDSA support, Snowflake, Databricks, Azure-specific paths, local CLI providers, Ollama discovery, and declarative provider loading.

4. **The embedded model catalog**
   - Do not embed `canonical_models.json`.
   - Accept an arbitrary model string, ship a tiny curated list, or fetch/cache catalog metadata at runtime.

5. **Remove scheduling; preserve session storage deliberately**
   - Remove `tokio-cron-scheduler`, scheduler services, cron parsing, scheduled recipe execution, and scheduler CLI commands.
   - Session persistence is required, so SQLx cannot simply be deleted without a replacement.
   - SQLx itself contributes about 527 KiB of measured `.text` (`sqlx-core` plus `sqlx-sqlite`), while bundled SQLite contributes about 96 KiB. It is meaningful but not one of the largest blockers to 10 MB.
   - The lowest-risk first version should retain SQLite but replace SQLx macros and its broad abstraction with a narrow storage boundary, potentially using `rusqlite` or direct SQLite bindings. Measure this before committing to a migration: the main benefit may be dependency/build simplification rather than multiple megabytes of final binary savings.
   - A file-backed JSONL/index implementation could be smaller, but only if it preserves ACP load/resume/fork, atomic updates, listing/filtering, metadata, extension state, and concurrent access semantics. Treat that as a storage redesign, not a simple size optimization.

6. **Rich CLI command surface**
   - Remove configure UI, doctor, review, recipes, plugins, skills management, terminal integration, completions, update machinery, and server commands.
   - Use a small Clap definition or hand-written argument parsing for `run`, provider, model, and extension options.

7. **Rich terminal presentation**
   - Remove Bat, Syntect, Oniguruma, Cliclack, Comfy Table, Indicatif, and advanced Markdown/syntax display.
   - Print text and tool events directly, with optional ANSI styling.

8. **Advanced agent facilities**
   - Remove dictation, image/document processing, subagents, hooks, skills, recipes, context compaction/token counting, platform apps, and state-machine functionality not needed by the minimal loop.
   - Retain only message accumulation, provider streaming, and a tool-call loop.

9. **Duplicate and unintended protocol stacks**
   - Ensure native-TLS builds contain no Rustls/AWS-LC path.
   - Use one Reqwest version and one TLS backend.
   - If OAuth is omitted, the Reqwest 0.12 path disappears as well.

### Architecture recommendation

Trying to place feature gates around the existing `goose` crate is possible, but its modules and public types are deeply interconnected. A cleaner route is a small core crate, for example `goose-lean` or `goose-agent-core`, containing:

- Minimal message and tool-call types
- A narrow provider trait
- One OpenAI-compatible HTTP provider
- The basic agent/tool loop
- Core ACP stdio handling
- The in-process developer extension
- A session-storage trait with a compact SQLite implementation
- Optional external stdio MCP client support

The existing `goose` crate can depend on and extend this core with sessions, ACP, bundled tools, provider inventory, recipes, scheduling, security UI, and desktop integration. The lean binary should not depend on the current `goose`, `goose-mcp`, or broad `goose-providers` crates, because merely selecting a small runtime path still instantiates and embeds a large amount of their product surface.

### Expected size progression

These are directional budgets that need to be validated after each feature boundary is implemented:

| Stage | Expected size |
|---|---:|
| Current size-oriented Goose | 32 MiB |
| Externalize model catalog and remove autovisualiser assets | 23–25 MiB |
| Remove bundled MCP/document servers | 18–21 MiB |
| Narrow ACP; remove gateways, scheduler, and secondary commands | 14–18 MiB |
| Replace universal providers and full agent crate with minimal core while retaining ACP, developer, and session storage | 7–12 MiB |

The 10 MB target remains plausible but is now tight. The final margin depends on how much of ACP's custom desktop surface is retained, whether external RMCP client support is included, and the chosen storage implementation. Core ACP plus developer should fit; full ACP management routes, every MCP transport, and the current session/search feature set probably will not all fit under a strict 10 MiB cap.

### Recommended first prototype

Build a new experimental binary with this contract:

```text
goose-lean run --provider openai --model MODEL [PROMPT]
goose-lean session --provider openai --model MODEL
```

Initial dependencies should cover Tokio, Reqwest, Serde/Serde JSON, Anyhow, a minimal CLI parser, ACP schema/transport, the developer tool implementation, and the selected session backend. Add streaming first, then developer tools, then core ACP, then persistence, and finally external stdio MCP client support, recording binary size after each addition. A hard CI threshold of 10 MiB will prevent the broad dependency graph from growing back.

## Conclusion

The approximately 50 MB target is achievable through compiler and linker configuration alone: the same native-TLS, no-default-features CLI falls from approximately 111 MiB to 32 MiB. A 10 MB target requires a separate minimal product architecture. The measured HTTP chat prototype shows that 10 MB is feasible, but only if the lean executable stops depending on the full Goose, bundled MCP, universal provider, ACP, SQLx, and rich terminal stacks.
