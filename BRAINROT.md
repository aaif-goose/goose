# BRAINROT — Why goose feels dumb at 200K characters

A field report from inspecting `~/.local/state/goose/logs/llm_request.*.jsonl` across
sessions on `claude-opus-4-5` / `claude-opus-4-6` / `claude-opus-4-7`, compared against
pi (same model) doing comparable tasks.

## Assessment

The "brain rot" is not a single bug. It is the **absence of a context-hygiene layer**
in the goose agent. Every tool result goose receives lands verbatim in the prompt,
regardless of size, regardless of duplication, regardless of staleness. The model is
then expected to think on top of a context where 85–90% of the bytes are tool output,
much of it dead weight.

Concretely, in one representative 51-message session (`dd77b013`):

| bucket                 | chars   | % of context |
| ---                    | ---     | ---          |
| tool_result bodies     | 385,804 | **86.5%**    |
| tool_use args (TS)     |  45,483 | 10.2%        |
| assistant reasoning    |   7,247 |  1.6%        |
| tool definitions       |   4,818 |  1.1%        |
| system prompt          |   2,012 |  0.5%        |
| user text              |     812 |  0.2%        |

Sub-1-percent of bytes is user intent. ~1.6% is model reasoning. The rest is tool
spew. At 200K+ chars of this, the model can no longer locate signal and starts to
confabulate / forget / repeat / drift. **Same model in pi does not do this** because
pi caps and dedupes aggressively.

This is not specific to code-mode. Code-mode amplifies it (bundling instructions
push fatter single results), but plain shell sessions hit the same wall, just slower.

## Mechanisms

1. **No per-tool-result size cap at the agent layer.**
   `shell` self-caps at 50KB / 2000 lines. Good. But everything else
   (`text_editor`, MCP servers, `code_execution` bundles) returns whatever it wants,
   and goose embeds it as-is.

2. **No file-content deduplication across turns.**
   Read `plan.md` at turn 4 → 48KB. Edit `plan.md` at turn 50 → 60KB echo of
   `plan.md`. Both copies sit in context simultaneously. Pi tracks `readFiles` and
   `modifiedFiles` across compactions to avoid this.

3. **Fat edit responses (`text_editor` style).**
   The current in-tree `developer.edit` returns `Edited <path> (N lines -> M lines)`.
   Lean. Good.
   But sessions that route through `code_execution` → `developer.text_editor`
   (external MCP) get `"The file ... has been edited, and the section now reads:\n<entire post-edit file>"`.
   Three observed instances of 30KB, 36KB, 60KB.

4. **`code_execution` "bundle everything" attractor.**
   Tool description literally says *"CRITICAL: Always combine related operations into
   a single execute_code call. WRONG: 2 calls. RIGHT: 1 call."* The model dutifully
   bundles 3 file reads into one mega-result. Bundling itself is fine; what's bad is
   that **no field in the bundled `{ ... }` is size-capped**.

5. **Manual `/compact` is a one-way lossy step.**
   The compaction summary is model-authored, then the continuation prompt says
   *"Do not mention that you read a summary or that conversation summarization
   occurred. Just continue the conversation naturally based on the summarized context."*
   That nudges the model to confabulate continuity rather than admit gaps. Any fact
   lost in the summary is now gone from "ground truth."

6. **Tool-pair summarization exists but doesn't trigger when you'd want.**
   `GOOSE_TOOL_PAIR_SUMMARIZATION` (default `true`) summarizes batches of 10 old
   tool-call/response pairs. Cutoff scales with context: 24 tool calls on 200K-ctx
   models, **120 calls on 1M-ctx Opus**. So on the model where bloat is worst per
   call (because Opus accepts mega-results), summarization fires latest. The
   summarization prompt is also extremely lossy: *"Reply with a single message…
   e.g. 'A call to github was made to get the project status'"*. That's a 95% lossy
   compression of a tool result. It's an emergency vent, not hygiene.

## What pi does that goose doesn't

- **Tool results truncated to 2000 chars during summarization serialization.**
  Pi's `serializeConversation()` aggressively truncates tool results when feeding
  them into the summarizer. Goose feeds the full content.
- **Cumulative file tracking across compactions.** `readFiles` / `modifiedFiles`
  lists are accumulated and threaded through summaries, so the next agent step
  knows what's already been seen without reading it again.
- **Convention-driven tool design.** Pi's `read` defaults to 2000 lines, `bash`
  caps at 50KB, `edit` returns just the confirmation. Goose's `developer` does
  the right thing too (now). The leak is in *external* MCPs and `code_execution`
  bundling.
- **Compaction with structured summary format.** Pi's compaction emits a structured
  template (Goal / Constraints / Progress / Decisions / Next Steps / Critical
  Context + `<read-files>` / `<modified-files>` blocks). Goose's compaction emits
  freeform model prose, which is harder to keep coherent over multiple compactions.

## Proposed improvements, in leverage order

Status markers as of this branch: ✅ implemented, 🟡 partial, ⏭️ deferred.

### P0 — universal tool-result size cap ✅

The single most impactful change. Intercept every `tool_response` at the point it
becomes a `MessageContent` and, if the text body exceeds a threshold, replace the
overflow with a truncation marker that tells the model exactly how to retrieve more.

- **Where:** `crates/goose/src/conversation/message.rs::MessageContent::tool_response_with_metadata`
  (single choke point — all 3 call sites flow through here).
- **Threshold:** 16 KB per tool-result text item by default. Configurable via
  `GOOSE_TOOL_RESULT_MAX_BYTES`. Off when set to `0`.
- **Behavior:** keep the first ~14 KB, append a marker:
  `\n\n[goose: truncated N bytes of tool output. Re-call with narrower
  parameters (e.g. line/limit, grep, head) to inspect the rest.]`
- **Side-effect of the marker:** the model learns from the prompt that it should
  ask for narrower reads. This is a *prompt-engineering* win on top of a bytes win.
- **Why this is safe:** the full content is already on disk (for shell) or has
  been observed by the model in this turn (the assistant text part can reference
  what mattered). The marker preserves the tool-call structure, so id pairing
  stays valid.

Implemented in this branch as a first cut (16 KB cap, toggleable).

### P1 — content-hash dedup of repeated tool results within a session ✅

If the exact same `text` block has already been emitted as a tool result
earlier in the conversation snapshot being sent, replace the body with
`[goose: identical to tool result at turn X (sha=...), N bytes elided,
tool=...]`. Cheap, big win for the "re-cat the same file 3 times" pattern.

Implemented in `crates/goose/src/conversation/dedup.rs`, applied in
`stream_response_from_provider` just before the conversation is handed to the
provider. Keyed by `sha256(tool_name + args_json + body)`, so any change in
arguments or in the file contents flows through unmodified. Bodies under
512 bytes are skipped (not worth a marker). Disable with
`GOOSE_TOOL_RESULT_DEDUP=0`. Session DB and on-screen view are untouched.

### P2 — `code_execution` result bundle size cap ✅

Apply per-field truncation inside the bundled
`Result: { foo, bar, baz }` object before it's serialized to text. If `foo`
is 40 KB, it becomes `<...[goose: truncated 32K bytes from this field]>`
while `bar` and `baz` survive intact. Operates *before* P0's whole-block
cap so one fat field doesn't crowd out small useful fields in the same
bundle.

Implemented in `crates/goose/src/agents/platform_extensions/code_execution.rs`
via `truncate_execute_output()` which intercepts `ExecuteOutput` and walks
`output` (`serde_json::Value`) plus the top-level `stdout`/`stderr` strings.
Default cap 8 KB per field. Tunable via `GOOSE_CODE_EXEC_FIELD_MAX_BYTES`
(0 to disable). Applied to both `execute_typescript` and `execute_bash`.

### P3 — soften the "bundle everything" prompt attractor ✅

The upstream `pctx_code_mode` tool descriptions are explicit attractors
toward bundling. Rather than fork upstream, append a goose-side
`CONTEXT HYGIENE (goose):` addendum to each `execute_typescript`
description (catalog / filesystem / sidecar variants) that:

- tells the model to bundle only when each field is small (~5 KB),
- tells it to call large-output tools on their own,
- tells it to return only the fields it actually needs,
- tells it to use `line`/`limit`/`head`/`tail`/`grep` instead of `cat <big-file>`.

Implemented in `with_goose_context_advice()` in `code_execution.rs`. Upstream
descriptions are preserved verbatim and the addendum follows; no schema
changes, no tool semantics change.

### P4 — structured compaction summary + file-tracking + honest continuation ✅

Three coordinated changes:

1. **Rewrote `prompts/compaction.md`** as an explicit section-based
   template (Goal / Constraints & Preferences / Progress (Done | In
   Progress | Blocked) / Key Decisions / Critical Context / Next Step)
   followed by `<read-files>` and `<modified-files>` blocks. Tells the
   summarizer that the summary is its only memory, to prefer concrete
   facts, and not to invent facts not in the transcript.

2. **`extract_file_history()`** in `context_mgmt::mod` walks tool
   requests and pulls `path` arguments out of read/write/edit-shaped
   calls. Deduplicated in first-seen order. The lists are threaded into
   `SummarizeContext` and rendered in the template so the summarizer
   gets explicit file hints to include in its output.

3. **Rewrote the three continuation prompts** (auto compact, tool-loop
   compact, manual `/compact`). The previous prompts said
   *"Do not mention that you read a summary or that conversation
   summarization occurred"* — a direct confabulation cue. New text tells
   the model the previous message is a structured summary, that
   originals are gone, and to admit gaps plainly rather than guess.

### P5 — improve tool-pair summarization ⏭️ deferred

Deferred on purpose. pi does not do tool-pair summarization at all — it
relies on (a) bounded tool outputs by convention, (b) full-conversation
compaction. Goose's tool-pair summarization is an emergency vent, not
hygiene, and the lossy "A call to X was made" summary it produces is
likely to make brain rot worse, not better, on the long-context models
where it would fire latest. Better to leave it gated by
`GOOSE_TOOL_PAIR_SUMMARIZATION` (default true) and recommend disabling
it for users who notice the artifact. P0–P4 + P6 attack the actual
root cause.

### P6 — `developer.read` tool with a default cap ✅

The `read` handler was added then removed (`f2ad0a852`) because ACP clients
delegate filesystem I/O and intercept `read` at the AcpTools layer. Non-ACP
sessions were left without a structured read tool, forcing
`shell({command:"cat ..."})`.

Restored on `DeveloperClient` with `FileReadParams` (already present in
`developer/edit.rs`). Default cap of 2000 lines applied via
`DEFAULT_FILE_READ_LIMIT`; on truncation the body is followed by a marker
showing the line range, total line count, and how many more lines exist,
with the suggestion to pass `line`/`limit`. `developer_instructions`
updated on both POSIX and Windows to tell the model to prefer `read` over
`cat`/`sed`/`type`/`Get-Content`. ACP layer is unchanged: when its
`fs_read` capability is on, `AcpTools` still intercepts `read`.

## Things considered and rejected

- **Auto-`/compact` more aggressively**: makes the model dumber faster, not
  smarter. Lossy compression of context is the root cause of brain rot, not its
  cure.
- **Disable `code_execution` for users**: too much breakage; better to fix the
  result emission shape (P2 + P3).
- **Tighter context cap (lower `GOOSE_AUTO_COMPACT_THRESHOLD`)**: same trap as
  more aggressive compaction.

## Notes from the audit

- Manual `/compact` continuation prompt actively encourages confabulation:
  *"Do not mention that you read a summary or that conversation summarization
  occurred."* Consider changing to: *"You are operating on a summary of an
  earlier conversation. If asked about specifics not in the summary, say so
  rather than guessing."*
- The default 0.8 auto-compaction threshold may be too aggressive on long-context
  models. With 1M context, compacting at 800K is fine, but the bigger issue is
  that *getting to 800K* via fat tool results means hundreds of useless bytes
  per useful byte. P0+P1+P2 attack that root cause; threshold tuning is downstream.
