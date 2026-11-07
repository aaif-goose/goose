---
sidebar_position: 92
title: Target Admission (SPILLWAY)
sidebar_label: Target Admission
---

goose can check every tool call the model proposes against a deterministic prompt-injection floor **before** it reaches the environment, and strip instruction-like spans from tool results **before** the model reads them. This implements the core idea of [SPILLWAY: Separating Target Admission from Sanitization to Defend Tool-Using Agents against Prompt Injection](https://arxiv.org/abs/2611.07222).

## How it works

A tool call carries *targets*: the recipient of a message, the account of a transfer, the URL of a download, the channel of a post. SPILLWAY admits a call only when every target value it carries that appears in tool output but not in the user's task — a *taint-novel* value — arrived in a typed field of a read-only tool's structured return, or is covered by a fixed delegation rule. A value the user wrote themselves is always admitted; a value copied from attacker-controlled text is refused.

Provenance is tracked from the conversation, with no model call:

- **The user's text** is the text of every user-role message. **Tool output** is the text of every tool result (text blocks plus the serialized structured return). A target value is taint-novel when one of its identity tokens — the emails and URLs it contains, or the whole value when it is at least 4 characters — appears in tool output but not in the user's text, compared casefolded with whitespace collapsed.
- **The typed identity index** is built from the structured returns of read-only tools only. A value enters the index only when its field is an identity format (email, URL, UUID, hostname, IP) or has a routing role (recipient, sender, participant, channel, and similar). Body, subject, and description text never enter the index. A read whose own arguments are taint-novel adds nothing to the index, so a steered lookup cannot launder a target.
- **Free-text repetition blocks the lookup**: a value that also appears in a free-text tool-output block cannot be admitted through the typed index, even when it is in the index, because the model may have copied it from the free text.
- **Delegation rules** admit values the lookup refuses: a fixed rule binding a typed field of a read-only tool to an argument of another tool, a target that carries a name the user wrote, or a URL whose domain the user named.

Two extra checks run before any of that: a grantee must be named in the user's text (an unnamed grantee blocks the call outright), and a destructive call is refused unless the user's text authorizes the action.

When a call is denied, the model gets a tool error naming the refused target and why, and is told to re-issue the call with values from the user's request — so it can retry with an admissible call.

The sanitizer runs at the single place tool results enter the conversation, so every result the model reads next has passed through it. It strips three kinds of text, replacing each removed span with `[removed by spillway sanitizer]`:

- forged chat-template turns (`<|im_start|> ... <|im_end|>`), standalone turn markers, and the benchmark's `<INFORMATION> ... </INFORMATION>` injection template — the delimited region is replaced and the text after it stays;
- tool text addressing the agent about its task or answer, a forged plain-text turn marker (`System:` / `[USER]` at line start), a dictated tool call, or an "important message" block — removed from the match to the end of the return, so the data before it stays;
- a claim that a payee's account has changed, with the account it puts forward.

Removed spans are kept as evidence and logged as a security event. Structured content is never rewritten: it is typed data, and the identity index is built from it.

## Configuration

Set `GOOSE_SPILLWAY_FILE` to the path of a JSON config file (environment variable or config file). Unset means no guard — behavior is unchanged.

If the file is set but cannot be loaded (missing file, invalid JSON), goose installs a deny-all guard instead of failing open: every tool call is denied with a message naming the load error. Fix the file or unset the variable to restore normal operation. Sanitization stays on while the guard is unavailable: it does not depend on the config file's contents.

## Example config

```json
{
  "targets": [
    {
      "tool": "email__send",
      "arguments": ["to", "cc"]
    },
    {
      "tool": "drive__share",
      "arguments": ["file_id"],
      "principals": ["grantee"],
      "grant": true
    },
    {
      "tool": "files__delete",
      "arguments": ["path"],
      "destructive": true,
      "authorize_words": ["delete", "remove"]
    }
  ],
  "read_only_tools": ["contacts__lookup", "email__list", "drive__list"],
  "delegations": [
    {
      "from_tool": "contacts__lookup",
      "from_field": "email",
      "to_tool": "email__send",
      "to_argument": "to"
    }
  ],
  "sanitize": true
}
```

## Config reference

**Targets** (`targets`) — one entry per tool whose calls carry identity targets:

| Field | Meaning |
| --- | --- |
| `tool` | Tool name, a `prefix*` glob, or `*` for every tool |
| `arguments` | Argument keys carrying the target of the call |
| `principals` | Argument keys naming a grantee — must be named in the user's text |
| `destructive` | Deletion-like tool: refused unless the user's text authorizes it |
| `grant` | Access-granting tool: a taint-novel target is never admitted |
| `authorize_words` | Words in the user's text that authorize a destructive call (default: `delete`, `remove`, `destroy`) |

Tools with no target entry are not gated: the floor checks target-bearing calls, not every call.

**Read-only tools** (`read_only_tools`) — tool names or `prefix*` globs whose structured returns may populate the identity index. A tool not listed here never contributes to the index, even when it returns structured data.

**Delegations** (`delegations`) — fixed rules binding a typed field of a read-only tool to an argument of another tool:

| Field | Meaning |
| --- | --- |
| `from_tool` | The read-only tool the value comes from |
| `from_field` | Dot-separated path into its structured return |
| `to_tool` | The tool the value may be passed to |
| `to_argument` | The argument key it may fill |

**Sanitize** (`sanitize`) — whether tool results are stripped of instruction-like spans. Defaults to true. Set it to false if the sanitizer's false positives bite in your deployment; admission still runs.

## What this is not

This is the admission floor and the sanitizer from SPILLWAY, not the full paper. There is no two-view judge: the paper recovers some refused calls through a local model judge, while here a refused call is denied outright with a reason telling the model to re-issue it with values from the user's request. The "resource owned by the user's own account" delegation rule is not implemented — it needs an account identity the guard does not have; use a fixed delegation rule for the same cases. The "name the user wrote" delegation rule is a heuristic (a capitalized, non-sentence-initial token from the user's text), not named entity recognition. Nothing here makes a model call: the floor is deterministic by design.

For declarative deny-rules over tool calls (argument predicates, history conditions, repair suggestions), see the Tool Policies guide. For per-tool allow/ask/deny permissions managed through the UI, see [Extension Allowlist](/docs/guides/allowlist). For the SDK, the same engine is available as `goose_agent::spillway` (`SpillwayConfig`, `SpillwayHook`, `sanitize_text`, `sanitize_call_tool_result`), which implements the `PreToolCallHook` trait and can be wrapped in `PreToolCallHookOperation` in any embedding.
