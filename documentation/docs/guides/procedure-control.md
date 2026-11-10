---
sidebar_position: 93
title: Procedure Control
sidebar_label: Procedure Control
---

goose can keep an agent on a procedure — a skill or SOP stated once at the start of a run — with a deterministic finite-state machine that lives in the runtime, not in the model's context. This implements the core idea of [Keeping LLM Agents on Procedure: Executable Finite-State Control for Long-Horizon Instruction Fidelity](https://arxiv.org/abs/2611.10884).

## How it works

A procedure is a small JSON file naming states; each state lists the tools allowed in it and the transitions out of it, and one or more states are terminal. The runtime enforces the procedure at the same three points as the paper:

- **Tool calls** — only the tools listed for the current state may run. Anything else is denied with a message naming the tools that are allowed in the current state, so the model can self-correct instead of drifting to a shortcut.
- **State transitions** — the state advances only on a transition the procedure defines, driven by the tool that ran and optionally by a simple match on its result (contains or equals a string, or success / failure). A call with no matching transition leaves the state unchanged.
- **End of turn** — ending the turn while the procedure is in a non-terminal state is denied, so the model keeps working the procedure instead of stopping early with the work half done.

The state is tracked per run: a new user request restarts the procedure from its initial state, while the state persists across every turn of one run — including turns resumed after the stop hook denied an early finish. The model is never asked to track the state, and nothing here makes a model call: the FSM is deterministic by design.

## Configuration

Set `GOOSE_PROCEDURE_FILE` to the path of a JSON config file (environment variable or config file). Unset means no procedure control — behavior is unchanged.

If the file is set but cannot be loaded (missing file, invalid JSON, or a config that fails validation), goose installs a deny-all runtime instead of failing open: every tool call is denied with a message naming the load error. Ending the turn is still allowed in that case, so the model can report the broken config and stop instead of being trapped. Fix the file or unset the variable to restore normal operation.

## Example config

A three-state account-processing SOP: triage each account, verify the ones that check out, then decide. The run can only end from `decide`.

```json
{
  "name": "account-processing",
  "initial": "triage",
  "states": {
    "triage": {
      "tools": ["accounts__list", "accounts__read"],
      "transitions": [
        {"on_tool": "accounts__read", "on_result": {"contains": "verified"}, "to": "verify"},
        {"on_tool": "accounts__read", "to": "triage"}
      ]
    },
    "verify": {
      "tools": ["accounts__verify"],
      "transitions": [
        {"on_tool": "accounts__verify", "on_result": {"success": true}, "to": "decide"}
      ]
    },
    "decide": {
      "tools": ["accounts__decide"],
      "terminal": true
    }
  }
}
```

In `triage` the model can only list and read accounts. Reading an account whose result mentions "verified" moves to `verify`; any other read stays in `triage` (the second transition is a self-loop). In `verify` only the verify tool may run, and only a successful verification moves to `decide`. In `decide` — the terminal state — only the decide tool may run, and the turn may end.

## Config reference

**Procedure** — top-level fields:

| Field | Meaning |
| --- | --- |
| `name` | Human-readable name, used in denial messages. Optional. |
| `initial` | State the procedure starts in when a run begins. Required. |
| `states` | States by name. At least one must be terminal. |

**States** — one entry per state:

| Field | Meaning |
| --- | --- |
| `tools` | Tool names, `prefix*` globs, or `*` for every tool, allowed in this state. A non-terminal state must list at least one; a terminal state may list none, which means no further tool calls — the run should end. |
| `transitions` | Transitions out of this state, tried in order; the first whose tool and result match fires. |
| `terminal` | Whether reaching this state completes the procedure (the turn may end here). Defaults to false. |

**Transitions** — one entry per transition:

| Field | Meaning |
| --- | --- |
| `on_tool` | Tool name or `prefix*` glob that triggers this transition. Must be among the state's own allowed tools — a transition on a tool the state never allows could never fire. |
| `on_result` | Optional match on the tool's result. All fields present must match. |
| `to` | State to move to. Must name a state in the procedure. |

**Result match** (`on_result`) — at least one field is required:

| Field | Meaning |
| --- | --- |
| `contains` | Result text contains this string (case-insensitive). |
| `equals` | Result text equals this string exactly, after trimming whitespace. |
| `success` | Whether the call succeeded (`true`) or failed (`false`). Errors and error-flagged results count as failures; a transport error counts as a failure too, and its message is the matched text. |

Result text is the tool result's text blocks (plus text resource contents) joined with newlines. Structured content is not matched: the FSM reads what the model reads.

## What this is not

This is the executable-FSM core of the paper, not the full paper. The paper's benchmark harness (SOP-Bench) and its pre-registered experiment are not included — there is no benchmark runner here, only the control. The paper's FSM is driven by a workflow engine with typed step outcomes; here transitions match on the raw result text and success flag, which is all a goose tool result exposes without a model call. Terminal states still enforce their own tool list: reaching a terminal state does not open the run up to arbitrary tools — list any wrap-up tools in the terminal state's `tools` if the run needs them after the procedure completes. A procedure applies to the run it was loaded for; a new user request restarts it from the initial state, and subagent runs each load their own fresh copy.

For declarative deny-rules over tool calls (argument predicates, history conditions, repair suggestions), see the Tool Policies guide. For per-tool allow/ask/deny permissions managed through the UI, see [Extension Allowlist](/docs/guides/allowlist). For the SDK, the same engine is available as `goose_agent::procedure` (`ProcedureConfig`, `ProcedureRuntime`, `kickoff_id`), which the pre-tool, post-tool, and stop hooks share through one `Arc<Mutex<ProcedureRuntime>>`.
