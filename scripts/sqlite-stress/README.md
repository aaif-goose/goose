# SQLite stress harness

A small load test for goose's session database (`sessions.db`), written for
[#12898](https://github.com/aaif-goose/goose/issues/12898). It runs several
`goose acp` sessions at once and records what SQLite is doing while they run,
so a fix can be judged by numbers instead of guesses.

It is a single Python file with no dependencies beyond the standard library.
Each run uses a fresh `GOOSE_PATH_ROOT` in a temp directory, so your real
sessions and config are never touched, and a built-in fake OpenAI-compatible
server answers every model call, so no API key or tokens are needed.

## Quick start

```bash
cargo build --release -p goose-cli
python3 scripts/sqlite-stress/harness.py run --goose ./target/release/goose
```

This sweeps 1, 4, 8 and 16 concurrent sessions with 20 turns each and prints a
table like this one (from a debug build, `--scenario readers --turns 10`):

```
   label  scenario   topology   N   ok  fail  turn p50  turn p95  commit p50  commit p95  probe busy  busy logs  WAL max  ckpt busy  db fds
--------  --------  ---------  --  ---  ----  --------  --------  ----------  ----------  ----------  ---------  -------  ---------  ------
baseline   readers  processes   1   10     0     0.12s     0.44s       0.8ms       1.1ms           0          0    1.4MB         0%       6
baseline   readers  processes   4   40     0     0.13s     0.83s       1.0ms       1.0ms           0          0    5.1MB         0%      20
baseline   readers  processes   8   80     0     0.18s     1.51s       1.5ms       3.9ms           0          0   10.0MB         0%      47
baseline   readers  processes  16  160     0     0.25s     2.97s       9.0ms      41.6ms           0          0   20.6MB         0%      96
```

One JSON file per session count lands in `./stress-results/`, with every turn
and every sample, for deeper digging.

## Scenarios

The issue asks to keep the kinds of pressure apart, so each is its own scenario:

| `--scenario` | What it adds on top of the N sessions |
|---|---|
| `writers` (default) | Nothing. Only concurrent sessions writing messages. |
| `readers` | `--readers` connections that each hold a read transaction open for `--reader-hold` seconds, over and over. Old snapshots stop checkpoints from finishing, so this is the "does the WAL grow because of readers" test. |
| `external` | Runs `goose session list --format json` every `--external-interval` seconds, like a second client (desktop, CLI) touching the same database. |
| `all` | All of the above together. |

`--topology processes` (default) starts one `goose acp` per session, like
several editor windows. `--topology shared` starts one `goose acp` that hosts
every session, like one editor with many chats. They stress different things:
cross-process locking versus one process's connection pool.

## What gets measured

| Column | Meaning |
|---|---|
| `ok` / `fail` | Turns that ended with `end_turn` vs. turns that errored or timed out. Failure messages are printed and saved. |
| `turn p50/p95` | Wall time of `session/prompt`, for successful turns. The fake LLM adds `--llm-delay-ms` per call. |
| `commit p50/p95` | Every sample, the harness times a tiny `BEGIN IMMEDIATE; INSERT; COMMIT` in its own `harness_probe` table. This is how long any writer waits for the write lock right now. It is a proxy measured from outside; goose's own commit latency would need tracing inside goose. |
| `probe busy` | Probe writes that got `database is locked` within `--probe-timeout`. |
| `busy logs` | Lines matching `database is locked` / `SQLITE_BUSY` in goose's stderr and log files. |
| `WAL max` | Largest size of `sessions.db-wal` seen. |
| `ckpt busy` | Share of samples where `PRAGMA wal_checkpoint(PASSIVE)` reported busy. The JSON also has `log` and `checkpointed` frame counts per sample. |
| `db fds` | Most file handles on `sessions.db` held by the goose processes at once, which is roughly the number of open pooled connections. Uses `/proc` on Linux and `lsof` on macOS. |

Note that the checkpoint probe really does checkpoint (it is PASSIVE, so it
never blocks anyone), and the write probe really does write. Turn either off
with `--no-checkpoint-probe` or `--no-write-probe` to see goose on its own.

## Comparing configurations

The point of the harness is to run the same load against a few setups (the
current pool, a smaller pool, shorter-lived readers, idle checkpointing) and
compare. Build each variant to its own binary, give each run a `--label`, and
summarize them together:

```bash
python3 scripts/sqlite-stress/harness.py run --goose /tmp/goose-baseline --label baseline --scenario readers
python3 scripts/sqlite-stress/harness.py run --goose /tmp/goose-pool2    --label pool2    --scenario readers
python3 scripts/sqlite-stress/harness.py summarize stress-results/*-readers-*.json
```

`--env KEY=VALUE` passes extra environment to the goose processes, for
variants that are switched by an env var (for example `RUST_LOG=sqlx=debug`).

## Useful options

```
--sessions 4,16       which session counts to sweep
--turns 50            prompts per session
--reply-chars 20000   bigger replies, faster WAL growth
--llm-delay-ms 0      remove model latency to maximize write pressure
--max-duration 300    cap each run
--keep                keep the temp goose root (db, WAL, logs) to inspect
```

Run `python3 scripts/sqlite-stress/harness.py run --help` for the full list.
