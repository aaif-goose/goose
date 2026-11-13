---
sidebar_position: 94
title: Write Integrity
sidebar_label: Write Integrity
---

goose can refuse write tool calls whose effect the user never stated, with a deterministic check that runs before the call executes — no model call, no security artifact written by the model. This implements the core idea of [CapLite: Prompt-Injection Defense for Small Language Model Agents without Model-Written Security Artifacts](https://arxiv.org/abs/2611.17115).

## How it works

Other enforcement defenses ask the agent's own model to write a security artifact — a program, a privilege policy, a dependency graph, a constraint list — that code then enforces. Small models cannot be trusted to write those artifacts, so write integrity derives its policy from the tool schemas instead: a JSON config file lists the write tools and, per tool, which arguments decide whom or what a write affects (recipient, amount, object, destination, path, ...). The runtime enforces the policy at the pre-tool-call hook, before the call runs:

- **Commit before reading results** — the check reads only the user's request (the user messages since the run's kickoff). Tool results are never consulted, so text injected into a tool result cannot add a write, and cannot change a write's recipient, amount, object, or destination.
- **Ground every deciding argument** — for a write tool call, every grounded argument's value must be stated in the user request (case-insensitive, whitespace-insensitive; amounts match across `$50`, `50`, and `50.00`). Anything else is denied.
- **Repair, not a dead end** — a denial names the offending argument and value and tells the model to restate the write explicitly, so the user can confirm it in their next message and the call can proceed. A missing grounded argument is denied the same way: a write whose recipient or amount is unspecified has no stated effect.

Non-write tools are unaffected: a tool the policy does not list is never checked.

## Configuration

Set `GOOSE_WRITE_POLICY_FILE` to the path of a JSON config file (environment variable or config file). Unset means no write-integrity enforcement — behavior is unchanged.

If the file is set but cannot be loaded (missing file, invalid JSON, or a config that fails validation), goose installs a checker that denies every tool call instead of failing open: without a policy goose cannot tell a read from a write, so nothing runs until the config is fixed. Fix the file or unset the variable to restore normal operation.

## Example config

A payments policy: sending money is a write whose recipient and amount must both be stated in the user request; anything under `filesystem__` is a write whose path must be stated.

```json
{
  "name": "payments",
  "writes": [
    {"tool": "payments__send", "grounded_arguments": ["recipient", "amount"]},
    {"tool": "filesystem__*", "grounded_arguments": ["path"]}
  ]
}
```

With this policy, "Please send $50 to Alice" admits `payments__send` with `recipient: "Alice"` and `amount: 50` — but not `recipient: "Mallory"` or `amount: 500`, even if a tool result somewhere in the conversation says to send $500 to Mallory instead. A denial reads like: "write integrity refused tool `payments__send`: grounded argument `recipient` has value `Mallory`, which the user did not state in their request."

## Config reference

**Policy** — top-level fields:

| Field | Meaning |
| --- | --- |
| `name` | Human-readable name, used in denial messages. Optional. |
| `writes` | The write tools. At least one is required. |

**Write rules** — one entry per tool:

| Field | Meaning |
| --- | --- |
| `tool` | Tool name, `prefix*` glob, or `*` for every tool. Each tool may be listed only once; overlapping globs are allowed and the first matching rule wins. |
| `grounded_arguments` | Argument names whose values must be stated in the user request. At least one is required, each name must be non-empty, and no name may repeat within a rule. |

## Grounding rules

Grounding is a deterministic text match, not a model judgment. The value and the user request are both lowercased with whitespace collapsed, and a value grounds only when it appears in the request with a non-alphanumeric character (or the start/end of the text) on both sides — so `Al` does not ground in `Alice`, and `50` does not ground in `500`, while multi-word values, paths, and email addresses match. Amounts are normalized across spellings: currency symbols (`$`, `€`, `£`, `¥`) and digit-grouping commas are stripped, and a numeric value is also tried in canonical form, so a call with `50.00` grounds in a request that says `$50`, and a call with `1000` grounds in `$1,000`.

Grounding is checked against the current run's user request only — the user messages since the latest kickoff. A follow-up user message supersedes the earlier request: if the user first says "send $50 to Alice" and then says "actually, send $50 to Bob", only Bob grounds a write. The user can always restate a value to ground it.

Values are matched as the text the tool would act on: strings as-is, numbers and booleans in their JSON spelling, objects and arrays in compact JSON form. A grounded argument that is missing, null, or empty is denied.

## What this is not

This is the write-integrity core of the paper, not the full paper. The paper's AgentDojo evaluation (13,935 attacked episodes, zero successful attacks) is not included — there is no benchmark runner here, only the enforcement. The paper derives its write set and grounded arguments from the tool schemas automatically; here the policy author lists them in the config, because goose tools do not carry a read/write classification in their schemas. Spelled-out amounts do not ground numeric values ("fifty dollars" does not ground `50`) — state amounts as digits in the request. And grounding is deliberately literal: it cannot tell that "the usual account" means Alice, only that the value `Alice` was stated. When a write needs judgment about what the user meant rather than what they stated, that judgment belongs in the conversation with the user, not in the check.

For the SDK, the same engine is available as `goose_agent::write_integrity` (`WritePolicyConfig`, `WriteIntegrityChecker`), which the pre-tool hook enforces through one `Arc<WriteIntegrityChecker>`.
