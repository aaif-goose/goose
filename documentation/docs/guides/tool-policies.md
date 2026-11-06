---
sidebar_position: 91
title: Tool Policies
sidebar_label: Tool Policies
---

goose can check every tool call the model proposes against a declarative policy **before** it reaches the environment — fail-closed guardrails with a repair path instead of a bare refusal. This implements the core idea of [LEANGUARD: Lean-Verified Guardrails for Tool-Using Agents](https://arxiv.org/abs/2611.02332).

## How it works

A policy is a JSON file with a list of deny-rules. Each rule names a tool (exact name, a `prefix*` glob, or `*`) and the conditions a matching call must satisfy:

- **Argument predicates** — conditions on the call's arguments: equality, membership, substring, regex, or "path must stay under this directory".
- **History conditions** — conditions on the calls that already executed in this session: a call must (or must not) have happened before, or at most N times.

A call is admitted only if every matching rule's conditions hold. Anything a rule cannot establish — a missing argument, an unparseable value — counts against the call, so evaluation fails closed. Calls no rule mentions are allowed: policies are deny-rules, not an allow-list.

When a call is denied, the model does not get a bare refusal. It gets a tool error naming the violated rule and its hint, plus a repaired call when one can be found by editing the denied call — so it can retry with an admissible call. History violations (e.g. "read the file before editing it") cannot be repaired by editing the call, so those denials carry only the hint.

## Configuration

Set `GOOSE_TOOL_POLICY_FILE` to the path of a JSON policy file (environment variable or config file). Unset means no guard — behavior is unchanged.

If the file is set but cannot be loaded (missing file, invalid JSON, bad regex), goose installs a deny-all policy instead of failing open: every tool call is denied with a message naming the load error. Fix the file or unset the variable to restore normal operation.

## Example policy

```json
{
  "rules": [
    {
      "name": "shell-stays-in-workspace",
      "tool": "developer__shell",
      "require": [
        { "type": "path_under", "key": "command", "dir": "/workspace" }
      ],
      "hint": "Run shell commands inside /workspace."
    },
    {
      "name": "read-before-write",
      "tool": "developer__text_editor",
      "history": [
        { "type": "once", "tool": "developer__read_file" }
      ],
      "hint": "Read the file before editing it, so the edit is based on its current contents."
    },
    {
      "name": "one-transfer-per-session",
      "tool": "bank__transfer",
      "history": [
        { "type": "never", "tool": "bank__transfer" }
      ],
      "hint": "Only one transfer per session; ask the user before a second one."
    }
  ]
}
```

## Rule reference

**Tool matching** — `tool` is an exact tool name, a `prefix*` glob (e.g. `developer__*`), or `*` for every tool.

**Argument predicates** (`require`):

| Predicate | Meaning |
| --- | --- |
| `{"type": "eq", "key": ..., "value": ...}` | Argument equals this JSON value |
| `{"type": "one_of", "key": ..., "values": [...]}` | Argument is one of these values |
| `{"type": "not_contains", "key": ..., "substring": ...}` | String argument must not contain this substring |
| `{"type": "matches", "key": ..., "pattern": ...}` | String argument must match this regex |
| `{"type": "path_under", "key": ..., "dir": ...}` | Path argument must stay under this directory (`..` cannot escape it) |

**History conditions** (`history`):

| Condition | Meaning |
| --- | --- |
| `{"type": "once", "tool": ...}` | A call to this tool must already have executed |
| `{"type": "never", "tool": ...}` | No call to this tool may have executed yet |
| `{"type": "at_most", "tool": ..., "count": N}` | At most N prior executions of this tool |

A request counts as executed once it has a successful (non-error) response, so denied or failed attempts do not count toward history conditions.

## What this is not

This is the policy-enforcement and repair loop from LEANGUARD, not the full paper. There is no Lean4 prover: the paper proves its decision procedure equivalent to a LeanLTL semantics, while here the decision procedure *is* the semantics, so a repaired call is "proved admissible" by re-evaluating it with the same procedure. Repair is a bounded syntactic search over edits the violated predicates suggest — not Lean metaprogramming over a term language.

For per-tool allow/ask/deny permissions managed through the UI, see [Extension Allowlist](/docs/guides/allowlist). For the SDK, the same engine is available as `goose_agent::tool_policy` (`ToolPolicy`, `PolicyGuardHook`), which implements the `PreToolCallHook` trait and can be wrapped in `PreToolCallHookOperation` in any embedding.
