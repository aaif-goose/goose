"""Load harbor 0.8.0 job/trial results and render list/show/task/compare reports."""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

from harbor.models.job.result import JobResult
from harbor.models.trial.result import TrialResult


RUNS_DIR = Path(__file__).resolve().parent / "runs"


@dataclass
class LoadedJob:
    summary: JobResult
    results: list[TrialResult]
    job_dir: Path

    @property
    def job_name(self) -> str:
        return self.job_dir.name

    @property
    def started_at(self):
        return self.summary.started_at


@dataclass(frozen=True)
class TokenTotals:
    input_tokens: int | None
    cache_read_tokens: int | None
    cache_write_tokens: int | None
    output_tokens: int | None
    cost_usd: float | None


def load_job(job_dir: Path) -> LoadedJob:
    summary = JobResult.model_validate_json((job_dir / "result.json").read_text())
    results: list[TrialResult] = []
    for child in sorted(job_dir.iterdir()):
        if not child.is_dir():
            continue
        trial_result = child / "result.json"
        if not trial_result.is_file():
            continue
        results.append(TrialResult.model_validate_json(trial_result.read_text()))
    return LoadedJob(summary=summary, results=results, job_dir=job_dir)


def trial_reward(trial: TrialResult) -> float | None:
    if trial.verifier_result is None or not trial.verifier_result.rewards:
        return None
    rewards = trial.verifier_result.rewards
    value = rewards.get("reward", next(iter(rewards.values())))
    return float(value)


def trial_error(trial: TrialResult) -> tuple[str, str] | None:
    if trial.exception_info is None:
        return None
    return trial.exception_info.exception_type, trial.exception_info.exception_message


def trial_duration(trial: TrialResult) -> float | None:
    if trial.started_at is None or trial.finished_at is None:
        return None
    return (trial.finished_at - trial.started_at).total_seconds()


def trial_token_totals(trial: TrialResult) -> TokenTotals:
    n_in, n_cache_read, n_out, cost = trial.compute_token_cost_totals()
    if trial.agent_result is not None:
        contexts = [trial.agent_result]
    else:
        contexts = [
            step.agent_result
            for step in trial.step_results or []
            if step.agent_result is not None
        ]

    cache_write_values = [
        context.metadata.get("cache_write_input_tokens")
        for context in contexts
        if context.metadata is not None
        and context.metadata.get("cache_write_input_tokens") is not None
    ]
    n_cache_write = sum(cache_write_values) if cache_write_values else None
    return TokenTotals(n_in, n_cache_read, n_cache_write, n_out, cost)


def _trial_dir(trial: TrialResult, job_dir: Path) -> Path:
    return job_dir / trial.trial_name


def trial_turns(trial: TrialResult, job_dir: Path) -> int | None:
    """Number of agent turns in a trial.

    Preferred source is ``agent/trajectory.json`` (harbor's standard format,
    one entry per agent step). Falls back to parsing harness-specific logs
    when the trajectory isn't present:

      * goose stream-json: count messages with role=assistant
      * pi log: count "turn_start" events
    """
    trial_dir = _trial_dir(trial, job_dir)
    trajectory = trial_dir / "agent" / "trajectory.json"
    if trajectory.is_file():
        try:
            data = json.loads(trajectory.read_text())
        except json.JSONDecodeError:
            data = None
        steps = data.get("steps") if isinstance(data, dict) else None
        if isinstance(steps, list):
            return sum(1 for s in steps if isinstance(s, dict) and s.get("source") == "agent")

    goose_log = trial_dir / "agent" / "goose.txt"
    if goose_log.is_file():
        # stream-json emits one `message` event per streamed chunk (sharing the
        # same message.id for a single assistant turn). Dedupe by id so a turn
        # that streamed 2000 tokens counts as 1, not 2000.
        seen_ids: set[str] = set()
        anon_chunks = 0
        for line in goose_log.read_text(errors="replace").splitlines():
            line = line.strip()
            if not line.startswith("{"):
                continue
            try:
                obj = json.loads(line)
            except json.JSONDecodeError:
                continue
            if obj.get("type") != "message":
                continue
            msg = obj.get("message", {})
            if msg.get("role") != "assistant":
                continue
            mid = msg.get("id")
            if mid:
                seen_ids.add(mid)
            else:
                anon_chunks += 1
        count = len(seen_ids) + anon_chunks
        return count if count else None

    pi_log = trial_dir / "agent" / "pi.txt"
    if pi_log.is_file():
        count = 0
        for line in pi_log.read_text(errors="replace").splitlines():
            line = line.strip()
            if not line.startswith("{"):
                continue
            try:
                obj = json.loads(line)
            except json.JSONDecodeError:
                continue
            if obj.get("type") == "turn_start":
                count += 1
        return count if count else None

    return None


def job_turn_totals(job: LoadedJob) -> int:
    return sum((trial_turns(t, job.job_dir) or 0) for t in job.results)


def job_token_totals(job: LoadedJob) -> TokenTotals:
    totals = [trial_token_totals(t) for t in job.results]
    return TokenTotals(
        input_tokens=sum((total.input_tokens or 0) for total in totals),
        cache_read_tokens=sum((total.cache_read_tokens or 0) for total in totals),
        cache_write_tokens=sum((total.cache_write_tokens or 0) for total in totals),
        output_tokens=sum((total.output_tokens or 0) for total in totals),
        cost_usd=sum((total.cost_usd or 0.0) for total in totals),
    )


def trial_status(trial: TrialResult) -> str:
    """Classify a trial as pass / partial / fail / timeout / error / no-reward.

    Reward wins over exception_info: harbor can record an AgentTimeoutError or
    other post-run exception even when the verifier already scored the trial as
    a pass (e.g. the agent finished the work then the harness crashed during
    teardown, or the agent timed out after writing the correct answer). If we
    got points, we got points — count them.
    """
    reward = trial_reward(trial)
    if reward is not None and reward > 0:
        return "pass" if reward >= 1.0 else "partial"
    err = trial_error(trial)
    if err is not None:
        error_type, _ = err
        if "timeout" in error_type.lower():
            return "timeout"
        return "error"
    if reward is None:
        return "no-reward"
    return "fail"


def job_duration(job: LoadedJob) -> float | None:
    """Total trial time, summed across all trials.

    This unrolls parallelism: a 4-hour run with 4 concurrent workers reports
    ~16h. We deliberately don't use elapsed job wall clock (min start → max
    finish) because that conflates "how long the benchmark took" with "how
    much concurrency I had on the host", making cross-run comparisons noisy.
    The sum is a stable measure of total compute.
    """
    durations = [d for d in (trial_duration(t) for t in job.results) if d is not None]
    return sum(durations) if durations else None


def job_model(job: LoadedJob) -> str:
    for trial in job.results:
        info = trial.agent_info
        if info and info.model_info and info.model_info.name:
            return info.model_info.name.rsplit("/", 1)[-1]
    return "?"


def task_name(trial: TrialResult) -> str:
    return trial.task_id.get_name()


def fmt_duration(sec: float | None) -> str:
    if sec is None:
        return "-"
    if sec < 60:
        return f"{sec:.0f}s"
    if sec < 3600:
        return f"{sec / 60:.1f}m"
    return f"{sec / 3600:.1f}h"


def fmt_tokens(n: int | None) -> str:
    if n is None or n == 0:
        return "-"
    if n >= 1_000_000:
        return f"{n / 1_000_000:.1f}M"
    if n >= 1_000:
        return f"{n / 1_000:.0f}k"
    return str(n)


def fmt_cost(usd: float | None) -> str:
    if usd is None or usd == 0:
        return "-"
    return f"${usd:.2f}"


def status_counts(trials: list[TrialResult]) -> dict[str, int]:
    counts = {"pass": 0, "partial": 0, "fail": 0, "timeout": 0, "error": 0, "no-reward": 0}
    for trial in trials:
        counts[trial_status(trial)] += 1
    return counts


def cmd_list(args: argparse.Namespace) -> int:
    if not RUNS_DIR.is_dir():
        print(f"No runs directory at {RUNS_DIR}", file=sys.stderr)
        return 1

    rows = []
    for child in sorted(RUNS_DIR.iterdir()):
        if not child.is_dir():
            continue
        if not (child / "result.json").is_file():
            continue
        job = load_job(child)
        counts = status_counts(job.results)
        total = len(job.results)
        rate = f"{100 * counts['pass'] / total:.1f}%" if total else "-"
        tokens = job_token_totals(job)
        breakdown = f"{counts['pass']}/{counts['fail']}/{counts['error']}/{counts['timeout']}"
        rows.append(
            (
                child.name,
                job_model(job),
                rate,
                fmt_duration(job_duration(job)),
                fmt_tokens(tokens.input_tokens),
                fmt_tokens(tokens.cache_read_tokens),
                fmt_tokens(tokens.cache_write_tokens),
                fmt_tokens(tokens.output_tokens),
                fmt_tokens(job_turn_totals(job)),
                fmt_cost(tokens.cost_usd),
                breakdown,
            )
        )

    if not rows:
        print(f"No jobs found in {RUNS_DIR}")
        return 0
    print(
        f"{'job_name':<40} {'model':<25} {'rate':>7} {'compute':>8} "
        f"{'in':>7} {'read':>7} {'write':>7} {'out':>7} "
        f"{'turns':>6} {'cost':>8} {'pass/fail/err/tout':>18}"
    )
    print("-" * 147)
    for row in rows:
        print(
            f"{row[0]:<40} {row[1]:<25} {row[2]:>7} {row[3]:>8} "
            f"{row[4]:>7} {row[5]:>7} {row[6]:>7} {row[7]:>7} "
            f"{row[8]:>6} {row[9]:>8} {row[10]:>18}"
        )
    return 0


def cmd_show(args: argparse.Namespace) -> int:
    job = load_job(RUNS_DIR / args.job_name)
    counts = status_counts(job.results)
    total = len(job.results)

    print(f"Job:          {job.job_name}")
    print(f"Model:        {job_model(job)}")
    print(f"Started:      {job.started_at}")
    print(f"Compute time: {fmt_duration(job_duration(job))}  (sum of trial durations)")
    print(f"Trials:       {total}")
    print(
        f"  pass={counts['pass']}  partial={counts['partial']}  fail={counts['fail']}  "
        f"timeout={counts['timeout']}  error={counts['error']}  no-reward={counts['no-reward']}"
    )
    if total:
        print(f"Pass rate:    {100 * counts['pass'] / total:.1f}%")
    tokens = job_token_totals(job)
    print(
        f"Tokens:       in={fmt_tokens(tokens.input_tokens)}  "
        f"cache-read={fmt_tokens(tokens.cache_read_tokens)}  "
        f"cache-write={fmt_tokens(tokens.cache_write_tokens)}  "
        f"out={fmt_tokens(tokens.output_tokens)}"
    )
    print(f"Turns:        {fmt_tokens(job_turn_totals(job))}")
    print(f"Cost:         {fmt_cost(tokens.cost_usd)}")
    print()
    print(
        f"{'task':<45} {'status':<10} {'reward':>7} {'dur':>7} "
        f"{'in':>7} {'read':>7} {'write':>7} {'out':>7} "
        f"{'turns':>6} {'cost':>7}  error"
    )
    print("-" * 153)
    for trial in sorted(job.results, key=task_name):
        status = trial_status(trial)
        if args.status and status != args.status:
            continue
        reward = trial_reward(trial)
        reward_str = f"{reward:.2f}" if reward is not None else "-"
        error = trial_error(trial)
        if error is not None:
            exception_class, message = error
            msg_first_line = (message or "").splitlines()[0] if message else ""
            err_str = f"{exception_class}: {msg_first_line}" if msg_first_line else exception_class
        else:
            err_str = ""
        if len(err_str) > 50:
            err_str = err_str[:47] + "..."
        tokens = trial_token_totals(trial)
        turns = trial_turns(trial, job.job_dir)
        turns_str = str(turns) if turns is not None else "-"
        print(
            f"{task_name(trial):<45} {status:<10} {reward_str:>7} "
            f"{fmt_duration(trial_duration(trial)):>7} "
            f"{fmt_tokens(tokens.input_tokens):>7} "
            f"{fmt_tokens(tokens.cache_read_tokens):>7} "
            f"{fmt_tokens(tokens.cache_write_tokens):>7} "
            f"{fmt_tokens(tokens.output_tokens):>7} "
            f"{turns_str:>6} "
            f"{fmt_cost(tokens.cost_usd):>7}  {err_str}"
        )
    return 0


def cmd_task(args: argparse.Namespace) -> int:
    job_dir = RUNS_DIR / args.job_name
    job = load_job(job_dir)
    matches = [t for t in job.results if task_name(t) == args.task_name]
    if not matches:
        names = sorted({task_name(t) for t in job.results})
        print(f"No task '{args.task_name}' in {args.job_name}.", file=sys.stderr)
        print(f"Available: {', '.join(names[:10])}{'...' if len(names) > 10 else ''}", file=sys.stderr)
        return 1

    for trial in matches:
        print(f"=== {trial.trial_name} ===")
        print(f"Status:       {trial_status(trial)}")
        print(f"Reward:       {trial_reward(trial)}")
        print(f"Duration:     {fmt_duration(trial_duration(trial))}")
        print(f"Started:      {trial.started_at}")
        print(f"Ended:        {trial.finished_at}")
        tokens = trial_token_totals(trial)
        print(
            f"Tokens:       in={fmt_tokens(tokens.input_tokens)}  "
            f"cache-read={fmt_tokens(tokens.cache_read_tokens)}  "
            f"cache-write={fmt_tokens(tokens.cache_write_tokens)}  "
            f"out={fmt_tokens(tokens.output_tokens)}"
        )
        turns = trial_turns(trial, job_dir)
        print(f"Turns:        {turns if turns is not None else '-'}")
        print(f"Cost:         {fmt_cost(tokens.cost_usd)}")
        error = trial_error(trial)
        if error is not None:
            exception_class, message = error
            print(f"Error class:  {exception_class}")
            for line in (message or "").splitlines()[:10]:
                print(f"  {line}")
        if trial.verifier_result and trial.verifier_result.rewards:
            rewards_str = ", ".join(f"{k}={v}" for k, v in trial.verifier_result.rewards.items())
            print(f"Verifier:     {rewards_str}")

        trial_dir = job_dir / trial.trial_name
        if trial_dir.is_dir():
            stdout_file = trial_dir / "verifier" / "test-stdout.txt"
            if stdout_file.is_file():
                lines = stdout_file.read_text(errors="replace").splitlines()
                if lines:
                    print("  verifier output (last 15 lines):")
                    for line in lines[-15:]:
                        print(f"    {line}")
            print(f"\nArtifacts in: {trial_dir}")
            agent_log = trial_dir / "agent" / "goose.txt"
            if not agent_log.is_file():
                agent_log = trial_dir / "agent" / "pi.txt"
            if agent_log.is_file():
                size = agent_log.stat().st_size
                print(f"  agent log: {agent_log.name} ({size:,} bytes)")
                if args.tail and size:
                    print(f"\n--- last {args.tail} lines of {agent_log.name} ---")
                    lines = agent_log.read_text(errors="replace").splitlines()
                    for line in lines[-args.tail:]:
                        print(line)
        print()
    return 0


def cmd_rm(args: argparse.Namespace) -> int:
    runs_dir = RUNS_DIR.resolve()
    targets: list[Path] = []
    for name in args.job_names:
        target = (RUNS_DIR / name).resolve()
        if runs_dir not in target.parents:
            print(f"refusing to remove path outside runs dir: {name}", file=sys.stderr)
            return 2
        if not target.is_dir():
            print(f"not a run directory: {target}", file=sys.stderr)
            return 1
        targets.append(target)

    for target in targets:
        size_kb = sum(p.stat().st_size for p in target.rglob("*") if p.is_file()) // 1024
        print(f"  {target.name}  ({size_kb:,} KB)")

    if not args.yes:
        prompt = f"Remove {len(targets)} run{'s' if len(targets) > 1 else ''}? [y/N] "
        if input(prompt).strip().lower() not in ("y", "yes"):
            print("aborted")
            return 1

    for target in targets:
        shutil.rmtree(target)
        print(f"removed {target.name}")
    return 0


def cmd_pull(args: argparse.Namespace) -> int:
    """Rsync runs from a remote into the local runs directory.

    ``remote`` should be ``user@host:/path/to/goose`` — we append
    ``/evals/harbor/runs/`` and pull into our own runs/.
    """
    remote = args.remote.rstrip("/")
    if ":" not in remote:
        print("remote must include host:path, e.g. tbench@douwe.com:/home/tbench/work/goose", file=sys.stderr)
        return 2
    source = f"{remote}/evals/harbor/runs/"
    RUNS_DIR.mkdir(parents=True, exist_ok=True)
    cmd = ["rsync", "-az", "--stats"]
    if args.delete:
        cmd.append("--delete")
    if args.jobs:
        for name in args.jobs:
            cmd.extend(["--include", f"{name}/", "--include", f"{name}/**"])
        cmd.extend(["--exclude", "*"])
    cmd.extend([source, str(RUNS_DIR) + "/"])
    print(" ".join(cmd))
    return subprocess.run(cmd, check=False).returncode


def cmd_compare(args: argparse.Namespace) -> int:
    job_a = load_job(RUNS_DIR / args.job_a)
    job_b = load_job(RUNS_DIR / args.job_b)
    a_by_task = {task_name(t): t for t in job_a.results}
    b_by_task = {task_name(t): t for t in job_b.results}
    only_a = sorted(set(a_by_task) - set(b_by_task))
    only_b = sorted(set(b_by_task) - set(a_by_task))
    common = sorted(set(a_by_task) & set(b_by_task))

    ca = status_counts(job_a.results)
    cb = status_counts(job_b.results)
    na, nb = len(job_a.results), len(job_b.results)

    print(f"A: {args.job_a}  ({job_model(job_a)})")
    print(f"B: {args.job_b}  ({job_model(job_b)})")
    print()
    print(f"{'metric':<18} {'A':>10} {'B':>10}  {'diff':>8}")
    print("-" * 50)

    def row(label: str, a: float | int, b: float | int, fmt: str = "{:.0f}") -> None:
        diff = b - a
        diff_fmt = fmt.replace("{:", "{:+", 1)
        print(f"{label:<18} {fmt.format(a):>10} {fmt.format(b):>10}  {diff_fmt.format(diff):>8}")

    row("trials", na, nb)
    row("pass", ca["pass"], cb["pass"])
    row("partial", ca["partial"], cb["partial"])
    row("fail", ca["fail"], cb["fail"])
    row("timeout", ca["timeout"], cb["timeout"])
    row("error", ca["error"], cb["error"])
    if na and nb:
        row("pass rate %", 100 * ca["pass"] / na, 100 * cb["pass"] / nb, "{:.1f}")

    a_tokens = job_token_totals(job_a)
    b_tokens = job_token_totals(job_b)
    print(
        f"{'tokens in':<18} {fmt_tokens(a_tokens.input_tokens):>10} "
        f"{fmt_tokens(b_tokens.input_tokens):>10}"
    )
    print(
        f"{'cache reads':<18} {fmt_tokens(a_tokens.cache_read_tokens):>10} "
        f"{fmt_tokens(b_tokens.cache_read_tokens):>10}"
    )
    print(
        f"{'cache writes':<18} {fmt_tokens(a_tokens.cache_write_tokens):>10} "
        f"{fmt_tokens(b_tokens.cache_write_tokens):>10}"
    )
    print(
        f"{'tokens out':<18} {fmt_tokens(a_tokens.output_tokens):>10} "
        f"{fmt_tokens(b_tokens.output_tokens):>10}"
    )
    print(f"{'turns':<18} {fmt_tokens(job_turn_totals(job_a)):>10} "
          f"{fmt_tokens(job_turn_totals(job_b)):>10}")
    print(
        f"{'cost':<18} {fmt_cost(a_tokens.cost_usd):>10} "
        f"{fmt_cost(b_tokens.cost_usd):>10}"
    )
    print(f"{'compute time':<18} {fmt_duration(job_duration(job_a)):>10} "
          f"{fmt_duration(job_duration(job_b)):>10}")

    if only_a or only_b:
        print()
        if only_a:
            print(f"Only in A ({len(only_a)}): {', '.join(only_a)}")
        if only_b:
            print(f"Only in B ({len(only_b)}): {', '.join(only_b)}")

    transitions: dict[tuple[str, str], list[str]] = {}
    for name in common:
        sa = trial_status(a_by_task[name])
        sb = trial_status(b_by_task[name])
        transitions.setdefault((sa, sb), []).append(name)

    same_pass = transitions.get(("pass", "pass"), [])
    same_not = [
        name
        for (sa, sb), names in transitions.items()
        if sa != "pass" and sb != "pass"
        for name in names
    ]
    a_only = [n for (sa, sb), ns in transitions.items() if sa == "pass" and sb != "pass" for n in ns]
    b_only = [n for (sa, sb), ns in transitions.items() if sa != "pass" and sb == "pass" for n in ns]

    print()
    print(f"Task-level comparison ({len(common)} shared tasks):")
    print(f"  both pass:          {len(same_pass)}")
    print(f"  both not-pass:      {len(same_not)}")
    print(f"  only A passes:      {len(a_only)}")
    print(f"  only B passes:      {len(b_only)}")

    if args.verbose:
        if a_only:
            print(f"\nOnly A ({args.job_a}) solved:")
            for name in sorted(a_only):
                print(f"  {name:<40} B={trial_status(b_by_task[name])}")
        if b_only:
            print(f"\nOnly B ({args.job_b}) solved:")
            for name in sorted(b_only):
                print(f"  {name:<40} A={trial_status(a_by_task[name])}")
    return 0
