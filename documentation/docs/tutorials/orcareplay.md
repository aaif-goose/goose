---
description: Record a goose session and re-run it against the recording, with no model calls
---

# Record and Replay with OrcaReplay

This tutorial covers how to record a goose session with OrcaReplay and then run it again against the recording — same conversation, same tool calls, and no model is called the second time. Nothing is installed into goose: OrcaReplay wraps the process from outside for that one run.

## What is OrcaReplay

[OrcaReplay](https://github.com/Continuum-AI-Corp/OrcaReplay) is an [open-source](https://github.com/Continuum-AI-Corp/OrcaReplay) (Apache-2.0) record/replay debugger for coding agents. It captures a run *below the harness* — the model traffic, the tool calls, and the per-turn file changes — keeping the verbatim request and response bytes rather than a summary. Because it keeps the bytes, the recording can be served back: the same run executes again with every model response coming from the trace instead of from a provider.

## Why OrcaReplay for goose

- **Nothing to configure in goose**: no plugin, no extension, no edits to `config.yaml`. OrcaReplay sets the provider variables for the child process it launches, and only for that process.
- **Free re-runs**: replaying a recorded session calls no model and spends no tokens. Useful when a session did something surprising and you want to look again without paying for it twice.
- **The whole run on one timeline**: model calls with token counts, tool calls with their arguments, and a git tree hash per turn.
- **Local by default**: traces are files under `.orca/` in your project. Nothing is uploaded.

## Set up OrcaReplay

```bash
npm i -g orcareplay
```

Requires Node 20+. Check it can record on your machine:

```bash
orca doctor
```

`agents detected` should include `goose`.

## Record a goose session

Put the provider and model in the environment — OrcaReplay passes them through rather than choosing for you — and run goose behind `orca record`:

```bash
export GOOSE_PROVIDER=openai
export GOOSE_MODEL=gpt-5.6-sol

orca record goose -- run -t "The test in test_calc.py fails. Fix the bug in calc.py."
```

goose runs normally and answers as it always would. When it exits, orca reports what it kept:

```
info recorded run=run_d5b6f43597c5 events=45 blobs=14 exit=0
```

Everything after `--` is passed to goose untouched, so any goose invocation works — `run -t`, `run -i`, a recipe, or a session name.

:::note
This example uses the `openai` provider. goose resolves that origin from `OPENAI_HOST` first and `OPENAI_BASE_URL` second, and OrcaReplay sets **both** at the same proxy so a value you already have exported cannot quietly win. For the `anthropic` provider it sets `ANTHROPIC_HOST`, which is the variable goose actually reads — it does not read `ANTHROPIC_BASE_URL`.
:::

## Read the session back

```bash
orca show last
```

```
run_d5b6f43597c5  goose@0.2.3  45 events  exit 0

SEQ  KIND   WHAT                          DETAIL
2    MODEL  gpt-5.6-sol                   1 messages
22   TOOL   shell                         ok
23   MODEL  gpt-5.6-sol                   11 messages
24   MODEL  gpt-5.6-sol                   stop: tool_use · 303 in · 64 out
25   TOOL   edit                          {"path":"calc.py","before":"def add(a, b): return a - b", …}
26   SNAP   tree f43fb0dfa638d29368b18…    0 changed
41   MODEL  gpt-5.6-sol                   stop: end_turn · 649 in · 42 out
```

`orca checkpoints last` lists the points the run can be forked from, and `orca export last -o run.html` writes a single self-contained file you can attach to an issue.

## Replay it against the recording

```bash
orca replay last
```

```
info replaying run=run_d5b6f43597c5 exchanges=8 egress=blocked
warn divergence seq=33 level=minor detail="request 6 differs by 1 char with an identical message count…"
info replay.done reused=8/8 exact=6 divergences=2 unmatched=0 exit=0
```

goose starts again, is served the recorded responses, and reaches the same answer without any model being called. `reused=8/8` means every recorded exchange was used.

:::warning
`egress=blocked` means **model-provider egress**, not network isolation. OrcaReplay refuses to
forward anything to a provider, so no model is called and no tokens are spent — but a replay still
executes the recorded tool calls for real. If a turn ran `curl`, or an MCP server that fetches
something, that request goes out over the ordinary network, because it never passes through
OrcaReplay's proxy. Replay is not a sandbox: to stop all egress, run it inside one.
:::

The two `minor` divergences are worth understanding rather than ignoring: goose puts a `<turn-context>` block in the prompt carrying `<current-time>` at minute granularity, so a replay that crosses a minute boundary differs from the recording by exactly one character per affected request. Replay inside the same minute reports `exact=8 divergences=0`. Nothing is waved through — anything that drifts is named.

## Fork onto a different model

From any checkpoint, a run can continue live on another model in an isolated git worktree:

```bash
orca compare last --models gpt-5.6-sol,gpt-5.5 --verify "python -m pytest"
```

Each model starts from byte-identical context rather than a re-typed prompt, and the `--verify` command's exit code is the verdict — so the output is which models fixed it, not three transcripts to read.

:::warning
A fork is a live agent, not a replay. From the fork point on, the model is really being asked and whatever it decides to do, it does — including running shell commands. Each fork gets its own worktree, so repository files are isolated; anything outside the tree is not.
:::

## Two limits worth knowing

**Shell exit codes are not captured.** goose resolves its shell without going through the PATH shim OrcaReplay installs, so `shell` tool calls appear in the trace with their output, but the real exit code, the duration, and the stdout/stderr split do not. The run says so at the end (`warn shell.ineffective`). Record with `--no-shell` if you would rather it claimed nothing than claimed that.

**A background call can end a strict replay early.** goose asks a second model to name the session while the conversation is already under way, so that call and the first real turn race, and the order is not the same twice. OrcaReplay matches them in either order, but the recording holds no answer for a call that was abandoned before the origin replied — that shows up as `unmatched=1`. `orca replay <run> --loose` carries the run through to the same answer.

## Learn more

- [OrcaReplay repository](https://github.com/Continuum-AI-Corp/OrcaReplay)
- [The goose adapter source](https://github.com/Continuum-AI-Corp/OrcaReplay/blob/main/packages/adapters/src/goose.ts) — the variables it sets, and why
- [What a bug hunt actually looks like](https://github.com/Continuum-AI-Corp/OrcaReplay#what-a-bug-hunt-actually-looks-like) — a worked example end to end
