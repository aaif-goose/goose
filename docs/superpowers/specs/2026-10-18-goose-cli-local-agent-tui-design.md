# Goose CLI local agent TUI design

**Status:** Spec approved
**Date:** 2026-10-18
**Audience:** People new to local LLMs, especially Ollama users
**Design branch:** `tui-local-llm-benchmark`

## Intent

Make Goose CLI an approachable, polished terminal workspace for local-model agents while keeping its provider flexibility and advanced agent capabilities. The first-run experience should help a newcomer explore available models before starting a conversation. During work, the TUI should make Goose’s answer, active model, context use, approvals, tools, and delegated tasks easy to understand.

Mistral Vibe is an interaction reference for command discovery, keyboard use, and visual clarity. Goose should develop its own visual language rather than copy Vibe’s branding or exact layout.

## User and success

The primary user is a newcomer exploring local models. They should be able to see what models Goose can use, understand which runtime each model comes from, select one without memorizing its ID, and start a useful task. They should be able to understand Goose’s actual answer without confusing it with tool activity, status, or hidden reasoning.

The experience succeeds when users can:

- Find the configured default and available local models from a single model picker.
- Tell whether a model is configured, installed locally, or available through Goose’s local-model catalog.
- Get model recommendations based on available system memory and avoid models that exceed an enforced resource budget.
- See which model and provider are active, and whether the provider is local or remote.
- Read the assistant’s final answer at a glance, with tool and delegated-agent activity available without overwhelming the conversation.
- Understand context use and compaction state, and respond to permission requests using Goose’s existing permission model.
- Discover actions through a searchable picker instead of memorizing command names.

## Product experience

### First run: model discovery

The interactive CLI opens on a model discovery screen rather than an empty prompt. It checks only configured or conventional local endpoints; it does not scan the network. Runtime checks should not block the interface. The first screen has:

- A **Configured** section, with Goose’s configured default clearly identified and labeled local or remote. The user can start with it directly.
- An **Installed locally** section, populated from connected runtime model lists such as Ollama and Goose’s local inference manager.
- A **Browse models** action for Goose’s existing local-inference catalog. Catalog access is on demand; local inventory remains visible if the catalog cannot be reached.

Selecting any configured or installed model opens a conversation. Selecting an item marked as downloadable shows its source, approximate download size when known, and an explicit download action with progress and cancellation where the backend supports them. A model pull or catalog download must never start merely because the user opened model discovery.

The configured default remains available even if local models are present. Goose must not silently replace a configured default. If no provider is configured, the screen explains that and leads into local model discovery first.

### Model picker

Typing `/model` or activating the model in the status header opens the same searchable picker. It lists:

1. Configured providers and models, including the configured default.
2. Models reported as installed by connected runtimes, including Ollama.
3. Models in Goose’s local-inference catalog, clearly marked as downloads.

Search filters the list as the user types. Each row labels its provider/source, local or remote status, and active/default state. The detail view shows context length, tool-use, vision, size, and performance details only when known. Metadata must distinguish provider-reported facts, catalog metadata, estimates, and unknown values. Switching models keeps the conversation and clearly warns when the new model has a smaller known context limit.

For local models, show a fit recommendation using currently available system RAM, GPU memory when it can be queried reliably, model/quantization metadata, and the requested context size. Do not use installed physical memory as if it were all free for inference. Refresh resource availability before model launch. Clearly distinguish **recommended**, **tight fit**, **over budget**, and **unknown** rather than promising a model will run based on rough estimates. The default budget uses currently available resources with a conservative platform-specific reserve; users can configure a stricter resource budget. Prevent selection when known model requirements exceed the effective budget. If fit is based only on uncertain estimates, warn and explain the uncertainty instead of hard-blocking. Resource guidance must not probe unrelated machines or upload system details.

### Conversation view

After model selection, the TUI presents one focused conversation view:

- A compact header shows workspace, active provider/model, local/remote status, approval mode, and context use.
- The conversation occupies the primary visual area.
- A persistent composer supports multiline input, `/` actions, and `@` path references.
- Tool calls and subagent activity appear as concise, expandable events linked to the result they produced.

Assistant answers use normal foreground contrast and clear Markdown hierarchy. Only secondary information such as timestamps, duration, and passive status may be dimmed. No answer text should inherit a disabled or thinking-style treatment. The renderer must not expose private chain-of-thought; it can show concise progress/status events and tool activity.

### Transcript visual language

The conversation is a flat, append-only transcript rendered without per-message labels or an enclosing box. Each user and assistant message is rendered as Markdown in reading order; the turn's meaning comes from the content and its position, not from a "You"/"Goose" label or a bordered "Conversation" panel. This keeps the surface close to a Mistral Vibe-style terminal while remaining goose's own visual language.

A live turn is shown inline at the tail of the transcript:

- While the model works and no answer text has arrived, a small braille pulse (⠋ ⠙ ⠹ ⠸, cycled per redraw) with a dimmed `Thinking…` label signals progress. The pulse is purely cosmetic and conveys no content.
- When an answer is streaming, its text renders inline with a trailing pulse while the turn is still open, then the pulse is dropped when the turn finishes.
- A turn that ends with no answer and no tool activity shows a dimmed `No response yet.` line.
- Tool and subagent activity render as compact secondary rows beneath the answer text, independent of whether an answer draft exists, so a running tool stays visible even before any answer text arrives.

Offer a plain accessible renderer and a non-alternate-screen fallback. The screen must remain usable in narrow terminals and when color is unavailable. Theme changes must not reduce answer contrast.

## Agent supervision

### Context

Keep context usage visible in the header and provide `/context` for a focused breakdown. Clearly separate provider-reported token counts from Goose estimates. Where an estimate is shown, describe its composition at a useful level (instructions, conversation, tool results, and loaded skills) without claiming exact per-item token counts unless available.

Reuse Goose’s existing automatic compaction and `/compact`. Add a preview of the intended compaction and optional preservation guidance as a later enhancement. Do not change current compaction thresholds or context policies as part of the first UI slice.

### Permissions and approvals

Show the current Goose mode in the header. Use existing Goose mode and `Permission` semantics for action prompts. Approval cards should state the tool and requested action in direct language, show the available decisions, and keep keyboard focus clear. The TUI must not create a more permissive policy than the session runtime enforces.

### Tools and delegated tasks

Show active tools and existing subagent events as compact activity rows with status and a short description. The user can expand an event to inspect its tool call/result. Show the model/provider used for a delegated task when that information is available. Offer cancel/retry only when the task runtime exposes those operations; otherwise explain the available control accurately.

Multi-agent fan-out is opt-in. The first TUI milestone does not change existing subagent behavior or add automatic parallelism for local models.

### Advisor

Goose does not currently have the advisor feature proposed in the earlier benchmark. Treat advisor as a later, separate capability: an explicit, read-only review action for a plan, response, or diff, using a user-selected/configured model. Clearly label the advisor’s provider and local/remote status. It must return findings without editing files. Advisor design and implementation require a separate scoped plan after the initial TUI and supervision foundations are reviewed.

## Command discovery and session actions

Typing `/` opens a fuzzy, searchable action picker. Entries include descriptions and source labels for built-in commands, skills, recipes, prompts, and available extension actions. Completion and the picker should share one command registry so help text does not drift from implemented actions.

Preserve Goose’s current in-session command names and semantics where possible. The first picker should expose existing `/help`, `/model`, `/mode`, `/status`, `/compact`, `/skills`, `/prompts`, `/prompt`, `/extension`, `/builtin`, `/edit`, `/clear`, `/new`, `/t`, `/r`, and exit actions. `/context` is proposed. `/model` is the primary newcomer path into model selection.

The existing CLI remains scriptable: `goose run` and its text/JSON/stream-json output are unchanged. The interactive TUI reuses existing session, configuration, provider, tool, and permission logic. Preserve an inline interactive fallback while the full-screen TUI is rolled out.

## Technical boundaries

The TUI belongs in the Goose CLI and should not replace the agent runtime or provider implementations. Use these conceptual components:

1. **Session controller:** owns the existing Goose agent/session and translates user actions into session input.
2. **Session event interface:** exposes assistant output, tool activity, permission requests, usage, provider/model changes, and subagent notifications as typed events instead of writing directly to the terminal.
3. **TUI application:** owns focus, navigation, command picker, model picker, transcript view, composer, and accessibility/inline fallback behavior.
4. **Model discovery service:** merges configured provider inventory, runtime-reported installed models, and the existing Goose local-inference catalog while retaining the source and confidence of each metadata field.
5. **Existing CLI paths:** continue using the same session/provider/configuration services, with `goose run` retaining its current output contract.

The current CLI session loop uses `rustyline`, direct printing, and terminal formatting in the same module as session coordination. The implementation plan should introduce a narrow event/action boundary incrementally rather than rewrite provider, session persistence, or agent behavior. Select a terminal UI library during implementation planning after checking workspace dependency policy and platform support.

## Privacy and failure behavior

- Discover installed local models through the configured local runtime; do not upload prompt or source content for model discovery.
- Make external catalog access and downloads explicit and user initiated.
- Label local and remote providers wherever models or delegated tasks are displayed.
- If Ollama is unreachable, show its configured endpoint and a concise diagnostic with a retry action; continue showing other configured/default models.
- If capabilities or context metadata are unavailable, show “unknown” and let the agent’s existing runtime behavior determine whether a task can proceed.
- If system or GPU memory information is unavailable, report that fit cannot be verified. Enforce a memory limit only when the limit and model requirement have a reliable basis; otherwise show an estimate and its uncertainty.
- If local inference lacks reliable tool calling, present failures plainly and offer chat-only use where the provider/model path supports it. Do not silently imply tool support.
- If the terminal lacks color or alternate-screen support, fall back to readable inline output with high contrast.

## Delivery slices

### Slice 1: model-first interactive TUI

- Model discovery landing screen with configured default, installed local models, and on-demand Goose catalog browsing.
- Searchable `/model` picker shared by the header control.
- Model source and capability confidence labels.
- Conversation renderer with answer-first contrast, compact activity, visible model/mode/context status, and accessible/inline fallback.
- Keep `goose run` and machine-readable output unchanged.

### Slice 2: context and permission supervision

- Context breakdown and clear reported-versus-estimated values.
- Visible current mode and readable permission cards using existing semantics.
- Model fit recommendations based on currently available memory, with an enforced budget for reliably over-budget models.
- Compaction preview and guidance if it can be added without changing compaction policy.

### Slice 3: session and orchestration supervision

- In-TUI session browser using existing list/resume/fork/rename/export operations.
- Expandable tool/subagent activity with inspection and supported task controls.
- Advisor is not included in this slice unless separately specified and approved.

## Non-goals

- Replacing or changing Goose’s provider, agent, permission, session storage, or recipe formats.
- Changing automatic context-compaction thresholds or silently switching models.
- Making Goose Ollama-only or hiding remote providers.
- Automatic model downloads, automatic parallel agents, or automatic advisor review.
- Implementing advisor and multi-agent orchestration as part of Slice 1.
- Recreating Mistral Vibe’s exact visual design.
- Changing `goose run` or its output format contract.
- Exposing raw private chain-of-thought. Any future reasoning UI shows status and concise summaries only, never the provider’s raw internal reasoning text, and must be a separate scoped plan.

## Deferred work

### Reasoning / thinking channel (Phase 2)

Phase 1 ships the flat transcript and cosmetic per-turn pulse with no dedicated reasoning event. A later slice may add a typed reasoning channel end to end: a `SessionEvent::Thinking` variant fed from the provider stream (the model layer already represents reasoning as `MessageContent::Thinking` / `redacted_thinking`), an accumulating draft on the live-turn state, and a collapsible `Thought ▸ …` row in the TUI like the Mistral Vibe reference. This is deferred because it touches the private chain-of-thought guardrail above and the provider reasoning stream is not yet surfaced into the TUI event boundary; it requires its own scoped plan and maintainers’ decision before implementation.

## Open implementation questions

These are implementation-planning decisions, not blockers for the product design:

- Which terminal UI library best fits the workspace’s platform, terminal, and dependency constraints?
- Which typed event/action interface can be extracted from the current CLI session loop with the smallest safe boundary?
- Which fields can be obtained reliably from Ollama runtime metadata versus Goose’s model catalog?
- Which platform APIs and model metadata support a reliable memory-fit estimate, and which systems need an “unknown” result?
- What terminal combinations will be used for contrast, narrow-width, SSH/tmux, and screen-reader verification?

## Repository context reviewed

- `crates/goose-cli/src/cli.rs` — command groups, session flags, and local-model commands.
- `crates/goose-cli/src/session/mod.rs` — interactive session coordination, direct terminal I/O, context reporting, permissions, and subagent notifications.
- `crates/goose-cli/src/session/input.rs` and `completion.rs` — current commands and completion behavior.
- `crates/goose-cli/src/session/output.rs` — Markdown, tool, subagent, usage, and context rendering.
- `crates/goose-providers/src/ollama.rs` — Ollama model listing and provider behavior.
- `crates/goose-local-inference/src/management.rs` — Goose-managed local model inventory and downloads.
- `documentation/docs/guides/sessions/smart-context-management.md` — current auto/manual compaction behavior.
- `documentation/docs/guides/goose-cli-commands.md` — documented CLI and in-session command surface.
- `documentation/docs/guides/context-engineering/subagents.mdx` — subagent concepts and usage.
