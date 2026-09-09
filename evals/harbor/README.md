# Harbor benchmark tooling for Goose

A small command-line tool for running and comparing terminal-bench-style
benchmarks against different agent harnesses, models, and goose builds.

## Current results

Latest `cmd.py list` snapshot across the runs in `runs/`. All `*-full` runs
cover the full `terminal-bench/terminal-bench-2` dataset (89 tasks).
`pass/fail/err/tout` is the per-status breakdown. `compute` is the sum of
per-trial durations (parallelism unrolled), not wall clock — it's a stable
measure of how much agent time a run cost regardless of host concurrency.
`turns` is the total number of agent turns across all trials (one per
assistant message / harness step).

```
job_name                            model                         rate  compute     in     out  turns     cost  pass/fail/err/tout
-----------------------------------------------------------------------------------------------------------------------------------
claude-sonnet46-full                claude-sonnet-4-6            55.1%    20.2h  102.3M    1.2M     3k   $42.83          49/23/1/16
goose-1.30-sonnet46-full            claude-sonnet-4-6            50.6%    23.7h    2.4M       -     3k        -          45/24/2/18
goose-sonnet46-full-code-mode       claude-sonnet-4-6            57.3%    22.0h   63.3M    1.1M     3k  $206.43          51/20/2/16
nemotron-full                       nemotron-3-nano-30b-a3b       1.1%    21.8h    9.5M    2.2M     1k        -           1/64/2/22
opencode-sonnet46-full              claude-sonnet-4-6            52.8%    22.2h  111.5M    1.6M     3k   $70.30          47/23/0/19
pi-sonnet46-full                    claude-sonnet-4-6            47.2%    24.4h  114.4M    1.8M     3k   $74.82          42/25/1/21
sonnet46-dev-only                   claude-sonnet-4-6            48.3%    23.2h   70.6M    1.2M     3k  $229.19          43/25/2/19
sonnet46-full                       claude-sonnet-4-6            50.6%    22.5h   62.4M       -     3k        -          45/21/3/20
sonnet46-sum_codem                  claude-sonnet-4-6            57.3%    21.9h   78.1M    1.4M     3k  $254.53          51/23/2/13
sonnet46-summon-full                claude-sonnet-4-6            55.1%    23.5h   67.2M    1.0M     3k  $217.28          49/19/3/18
```

Quick read:

- `goose-sonnet46-full-code-mode` and `sonnet46-sum_codem` (both run codemode,
  the latter also enabling summon) lead at **57.3%**.
- Stock goose (`sonnet46-full`, `developer,todo`) lands at **50.6%**, roughly
  on par with `opencode` (52.8%) and ahead of `pi` (47.2%) on the same model.
  Notably, `pi` also burned the most compute (24.4h) — slowest *and* lowest
  scoring of the sonnet runs.
- `claude-sonnet46-full` at **55.1%** is harbor's vanilla `Goose` harness
  (curl-installed) — useful sanity check that our `GooseBinaryAgent` adapter
  isn't leaving points on the floor.
- `nemotron-full` solves 1 task using roughly the same compute budget but
  only ~1k turns (vs 3k for sonnet runs) — the small model gives up or
  loses tool-call structure earlier, so it doesn't even reach the
  100-turn cap on most trials.

## Setup

Requires `uv`, Docker, and `rsync` on the host. `cmd.py` is a
[PEP 723 inline-uv script](https://peps.python.org/pep-0723/), so `uv` installs
its Python deps (just `harbor` and `PyYAML`) on first run.

Secrets live in a `.env` file. `cmd.py` looks for one in the current working
directory first, then in this script's directory. Only the keys for the
provider you're using need to be set:

```
ANTHROPIC_API_KEY=sk-ant-...
OPENROUTER_API_KEY=sk-or-...
DATABRICKS_HOST=https://...
DATABRICKS_TOKEN=...
OPENAI_API_KEY=sk-...
```

alternatively, you can just export them in the session where you run the benchmark

## Running a goose benchmark

The `run` subcommand builds a harbor config that uses our `GooseBinaryAgent`
adapter — it uploads your local goose binary into each task container,
generates a `config.yaml` from the template with the requested extensions
flipped on, runs the recipe, and streams JSON output.

```bash
# Pin a specific binary, default everything else
./evals/harbor/cmd.py run /path/to/goose --job-name my-run

# Different model
./evals/harbor/cmd.py run /path/to/goose \
  --model anthropic/claude-opus-4-5 --job-name opus-run

# OpenRouter
./evals/harbor/cmd.py run /path/to/goose \
  --model openrouter/nvidia/nemotron-3-nano-30b-a3b \
  --job-name nemotron-smoke

# Subset of tasks (note: harbor wants the qualified form)
./evals/harbor/cmd.py run /path/to/goose \
  --tasks terminal-bench/fix-git,terminal-bench/extract-elf \
  --job-name smoke

# Toggle which extensions are enabled in config.yaml
./evals/harbor/cmd.py run /path/to/goose \
  --extensions developer,todo,codemode --job-name codemode-run

# Double the per-task timeout (useful for rerunning AgentTimeoutError trials)
./evals/harbor/cmd.py run /path/to/goose \
  --timeout-multiplier 2.0 \
  --tasks terminal-bench/oom,terminal-bench/compile-vim \
  --job-name oom-retry-2x
```

Defaults:
- dataset: `terminal-bench/terminal-bench-2`
- model: `anthropic/claude-sonnet-4-6`
- extensions: `developer,todo`
- concurrency: 4
- max turns: 100
- trials: 1
- installs `libgomp1` in each container (disable with `--no-install-goose-runtime-deps`)

Use `--dry-run` to print the generated harbor config without launching.

## Running a non-goose harness

Stock harnesses that harbor ships with (opencode, pi, aider, claude-code, ...)
don't need our adapter — they install themselves in the container and read
secrets from env. Write a harbor YAML config directly and call `harbor run`:

```yaml
# opencode-sonnet46-full.yaml
job_name: opencode-sonnet46-full
jobs_dir: /path/to/goose/evals/harbor/runs    # so cmd.py picks it up
n_attempts: 1
n_concurrent_trials: 4
environment:
  type: docker
  force_build: false
  delete: true
  env:
    - ANTHROPIC_API_KEY=${ANTHROPIC_API_KEY}
agents:
  - import_path: harbor.agents.installed.opencode:OpenCode
    model_name: anthropic/claude-sonnet-4-6
datasets:
  - name: terminal-bench/terminal-bench-2
```

```bash
export ANTHROPIC_API_KEY=...
uv tool install harbor
harbor run -c opencode-sonnet46-full.yaml
```

The output lands under `evals/harbor/runs/opencode-sonnet46-full/`, alongside
goose runs. `cmd.py list / show / compare` treats them identically — they're
all harbor `TrialResult` JSON under the hood.

For pi specifically you can lift the existing config we used:

```yaml
agents:
  - import_path: harbor.agents.installed.pi:Pi
    model_name: anthropic/claude-sonnet-4-6
    kwargs:
      thinking: "off"
```

## Inspecting results

`cmd.py list` shows every run under `runs/` with one line per job:

```bash
./evals/harbor/cmd.py list
```

Drill into a specific run:

```bash
./evals/harbor/cmd.py show <job_name>                  # all tasks
./evals/harbor/cmd.py show <job_name> --status error   # filter by outcome
./evals/harbor/cmd.py show <job_name> --status timeout
```

Drill into a single task in a single run:

```bash
./evals/harbor/cmd.py task <job_name> <task_name>
./evals/harbor/cmd.py task <job_name> <task_name> --tail 50   # tail agent log
```

Compare two runs head-to-head:

```bash
./evals/harbor/cmd.py compare <job_a> <job_b>           # summary
./evals/harbor/cmd.py compare <job_a> <job_b> -v        # plus per-task diffs
```

Delete runs:

```bash
./evals/harbor/cmd.py rm <job_name> [<job_name> ...]    # confirms by default
./evals/harbor/cmd.py rm <job_name> -y                  # skip the prompt
```

## Syncing runs between machines

If you run benchmarks on a remote box and want to inspect them locally:

```bash
# Pull everything
./evals/harbor/cmd.py pull tbench@douwe.com:/home/tbench/work/goose

# Just specific jobs
./evals/harbor/cmd.py pull tbench@douwe.com:/home/tbench/work/goose \
  --jobs sonnet46-full pi-sonnet46-full

# Mirror exactly (delete local runs that aren't on the remote)
./evals/harbor/cmd.py pull tbench@douwe.com:/home/tbench/work/goose --delete
```

The remote argument is `user@host:/path/to/goose` — `pull` appends
`evals/harbor/runs/` to it and rsyncs into the local `runs/`.

## A typical comparison workflow

```bash
# Run two configurations on the remote (in screen / mosh / tmux)
ssh tbench@douwe.com
cd /home/tbench/work/goose
./evals/harbor/cmd.py run ./target/release/goose --job-name baseline
./evals/harbor/cmd.py run ./target/release/goose \
  --extensions developer,todo,codemode --job-name codemode

# Pull results locally
./evals/harbor/cmd.py pull tbench@douwe.com:/home/tbench/work/goose \
  --jobs baseline codemode

# Diff
./evals/harbor/cmd.py compare baseline codemode -v
```

For deeper per-task understanding (why did A pass and B fail on this one
task?), see the `compare-tasks` skill under `.agents/skills/`. Delegate to
it with the two job names and a task name and it will read both
trajectories, the task spec, and the verifier output, then explain the
mechanism behind the divergence.

