---
name: verify-tool-discovery
description: Model-check goose's progressive tool discovery (deferTools sessions, load_tools, search_tools, the 128-tool budget, loaded-tool order) with TLA+. Use when a change touches tool_discovery.rs, ops_tool_discovery.rs, the tool sort in prepare_inference_tools, ToolDiscoveryState, /clear, or how inference_tools filters deferred tools.
---

# Verify progressive tool discovery

`ToolDiscovery.tla` is a TLA+ model of one goose session that defers tools. TLC explores every order of these steps, from the first request on:

- a request is sent, and the model answers with up to two `load_tools` calls and one tool call;
- goose answers the loads and the call;
- `/clear` runs;
- goose restarts before answering;
- an extension is turned on or off.

`check.sh` runs each rule on its own and compares the result with the list of expected results at the top of the script.

## Rules

| Rule | In one sentence | Harm if it breaks |
| --- | --- | --- |
| `NoLoadPushesPastLimit` | A load never takes the next request past `MAX_ADVERTISED_TOOLS`, even with several `load_tools` calls in one response. | OpenAI rejects every later request until `/clear`. |
| `LoadsOnlyAppend` | When no extension changed and `/clear` didn't run, the previous request's tools are a prefix of the next request's. | Each load invalidates the cached tool prefix. |
| `LoadedToolsAreSent` | A tool the model was told is loaded, or told to call again next time, is in the next request. | The model loops on "not loaded". |
| `UnsentToolsNeverRun` | A tool runs only if the request that called it sent it, so one loaded in the same response doesn't run. | The model calls a tool whose schema it never saw. |
| `LoadsAreDistinct` | `loaded_tools` names each tool once. | The state grows, and order and budget math drift. |
| `EveryRequestFits` | No request carries more than `MAX_ADVERTISED_TOOLS` tools. | Same as the first rule. **Known finding: breaks.** |

### Known finding: `EveryRequestFits`

The budget is checked only when a tool loads. A tool loaded before its extension was turned off stays in `loaded_tools`. When the extension comes back, the tool returns with it, and no budget check runs.

The shortest path, as `/statechart` tells it:

1. Load `github__list_prs`.
2. Turn github off.
3. Load `linear__*`. The budget counts only the request without github, so the load fits.
4. Turn github back on.
5. The next request carries 8 tools when the limit is 7.

`replay_an_extension_returning_with_loads_skips_the_budget` in `crates/goose/src/agents/state_machine/tests/tool_discovery_replay.rs` replays the same path on the real pipeline. Its requests carry 8, 9, 7, 13 and 15 tools; the last load was accepted at 13. Turning extensions on without any load can also pass the limit, and that happens in sessions that don't defer tools, too. This rule is expected to break until the code changes. When a fix lands, change `EveryRequestFits breaks` to `holds` in `check.sh`.

## When to run it

Run it when a diff touches anything under "Coverage" below. Also run it when it changes what a request carries: which operations add tools (`inference_tools`), `MAX_ADVERTISED_TOOLS`, or `RESERVED_OPERATION_TOOLS`.

## Run

Once per machine, download TLC v1.7.4 into `~/.cache/goose-verify`. The script checks its sha256. It needs Java 11 or later.

```bash
.agents/skills/verify-tool-discovery/check.sh --install
```

Then run the check, which takes about 30 seconds:

```bash
.agents/skills/verify-tool-discovery/check.sh
```

Each line prints `ok` or `UNEXPECTED`, then the rule, what TLC found, and what was expected. The script exits 0 when every rule matches the expected result. Set `OUT=dir` to keep the TLC output in a known folder. `TLA2TOOLS` and `JAVA` override the jar and the java binary.

Run the replays on the real code:

```bash
source bin/activate-hermit && GOOSE_DISABLE_KEYRING=1 cargo test -p goose --lib tool_discovery_
```

### Read a result

Read every result with the pstack `/statechart` skill. Pass it `ToolDiscovery.tla`, `ToolDiscovery.cfg` and the view in `view.ts`. It turns each broken rule into its shortest path, in the words above. The model has about 16,000 states, so the chart draws only the bug paths.

When a rule breaks unexpectedly, decide whether the code changed or the model is wrong:

1. Replay the path in the real pipeline, one test step per action. Use `tool_discovery_replay.rs` and `tool_discovery_lifecycle.rs` as patterns: `DummyApi::on(..).call(..)` for a model answer, `remove_extension` or `attach` for extension changes, and `pipeline.run([..])` to drive a turn.
2. If the real code breaks the same way, it's a bug. Report it with the path, and don't weaken the rule or the model.
3. If the real code doesn't break, the model is wrong. Fix the action whose code it describes, using the code pointers in the spec's comments, and rerun.

## Coverage

The model was last confirmed against **commit `9286975dc`**, the PR 2 head (aaif-goose/goose#13355), which includes PR 1 (aaif-goose/goose#13354). Move this commit forward only after you have read the code below at the new commit and confirmed that the model still describes it.

| Model part | Code |
| --- | --- |
| `Deferrable`, `AlwaysLoad` | `ExtensionLease::deferrable_tools` in `extension_manager/lease.rs`, and `always_loads` in `agents/tool_discovery.rs` |
| `Offered` | `ToolExecutionOperation::inference_tools` in `state_machine/ops_toolcalling.rs`, plus `inference_tools` of `ToolDiscoveryOperation`, `SkillOperation` and `RecipeOperation` |
| `Key`, `SortBy`, `Request` | `prepare_inference_tools` in `agents/reply_parts.rs`, and `admission_order` in `tool_discovery.rs` |
| `Matches`, `Resolve` | `resolve_load` in `tool_discovery.rs` |
| `RunLoads`, `DiscoveryRun` | `ToolDiscoveryOperation::run` and `load` in `state_machine/ops_tool_discovery.rs` (`loaded_this_batch`, `advertised_tool_count`) |
| `CallAnswered` | `request_was_advertised` and the known-tools filter in `ToolExecutionOperation::run` (`ops_toolcalling.rs`), and `not_loaded_message` in `RemainingToolCallsOperation` (`ops_remaining_tool_calls.rs`) |
| `Clear` | `CompactionOperation::clear` in `state_machine/ops_compaction.rs` |
| `LeaseLost` | `EXPIRED_APPROVAL_RESPONSE` paths in `ops_tool_discovery.rs` and `ops_toolcalling.rs`, and `prepare_session` in `state_machine/inference_preparation.rs` (the only writer of the shared lease) |
| `ToggleExtension` | an extension enabled or disabled between requests (`ExtensionManager::enable`/`disable`) |

## What it does not cover

- **Bounds.** The model has 2 extensions with 4 tools: 1 always-load and 3 deferrable. It also has 1 platform tool, 3 operation tools, and a limit of 7, scaled down from 128 with the same headroom. A response holds at most two `load_tools` calls and one tool call. TLC explores every state within these bounds and stops there.
- **Not modeled:**
  - `search_tools` ranking. It only reads state.
  - Mangled-name recovery (`deferred_tool_named`).
  - Code mode, which turns deferral off.
  - Chat mode, approvals and hooks.
  - Recipes' `final_output`.
  - Sessions that don't defer tools.
  - Several sessions.
- **Compaction and reconstruction** are treated as steps that change none of the variables. That holds because every variable is saved session state, which `loaded_tools_survive_auto_compaction` and `loaded_tools_survive_reconstruction` test. Turning extensions off while tool calls wait is modeled only as a restart.
- **System prompt invariance across loads** is not in the model. `TOOL_DISCOVERY_NOTE` depends only on whether the session defers tools. `loads_do_not_change_the_system_prompt` and `the_discovery_note_depends_only_on_deferring` test it.

## Change the model when the code changes on purpose

Edit the action that describes the changed code, keeping each action one indivisible step of the code. Then check that the rules can still fail by breaking each guard in a copy of the model and running `check.sh COPY.tla`. Each copy must break the rule it guards:

| Guard | Change | Rule that must break |
| --- | --- | --- |
| batch budget | `Len(sent) + loadedThisBatch` → `Len(sent)` | `NoLoadPushesPastLimit` |
| load order | `Key(t, ld) == <<0, 0, t>>` | `LoadsOnlyAppend` |
| rejected loads | `told \|-> IF fits THEN m ELSE {}` → `told \|-> m` | `LoadedToolsAreSent` |
| advertised gate | drop `call \in Range(sent) /\` from `runs` | `UnsentToolsNeverRun` |
| dedupe | drop `/\ t \notin Range(ld)` from `newly` | `LoadsAreDistinct` |

The config's `CONSTRAINT Bounded` cuts off only states that a broken `resolve_load` reaches. Without it, the dedupe mutant never finishes.
