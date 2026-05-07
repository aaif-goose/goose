#!/usr/bin/env python3
"""slices.py — read pass-rate breakdowns out of the eval-bench SQLite store.

Three subcommands:

    runs                List the most recent runs (newest first), one per line,
                        with headline pass^k.

    show <run_id>       For one run, print the overall pass@k / pass^k plus
                        a per-axis breakdown. The CLI face of the Slice
                        Explorer.

    compare <a> <b>     Diff two runs across shared axes — what got better,
                        what got worse, what's new or missing.

Defaults: reads from ~/.skein/eval-bench.sqlite. Override with --store.
Filter `runs` by --recipe; filter `show` and `compare` by axis name(s).

Exit codes:
    0  ran successfully (regardless of pass / fail content of the run).
    2  setup error (missing store, bad run id, bad arguments).
"""

from __future__ import annotations

import argparse
import sys
from collections.abc import Iterable
from pathlib import Path

_THIS_DIR = Path(__file__).resolve().parent
if str(_THIS_DIR) not in sys.path:
    sys.path.insert(0, str(_THIS_DIR))

from lib import ResultsStore  # noqa: E402
from lib.kpass import TrialResult, compute_passk, passk_by_slice  # noqa: E402
from lib.store import DEFAULT_STORE_PATH, RunRow, TrialRow  # noqa: E402


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--store",
        type=Path,
        default=DEFAULT_STORE_PATH,
        help=f"Path to the SQLite store. Default: {DEFAULT_STORE_PATH}",
    )
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_runs = sub.add_parser("runs", help="list recent runs")
    p_runs.add_argument("--limit", type=int, default=20)
    p_runs.add_argument("--recipe", default=None, help="filter to one recipe path")

    p_show = sub.add_parser("show", help="break down one run by axis")
    p_show.add_argument("run_id", type=int)
    p_show.add_argument(
        "--axis",
        action="append",
        default=[],
        help="axis name to slice by; repeat for multiple. Default: every recorded axis.",
    )

    p_compare = sub.add_parser("compare", help="diff two runs")
    p_compare.add_argument("run_a", type=int)
    p_compare.add_argument("run_b", type=int)
    p_compare.add_argument("--axis", action="append", default=[])

    args = parser.parse_args(argv)

    if not args.store.exists():
        print(
            f"error: store {args.store} does not exist. "
            "Run a recipe with `--store <path>` (run_kpass.py / run_once.py) "
            "or use the default ~/.skein/eval-bench.sqlite.",
            file=sys.stderr,
        )
        return 2

    store = ResultsStore(path=args.store)

    if args.cmd == "runs":
        return _cmd_runs(store, limit=args.limit, recipe=args.recipe)
    if args.cmd == "show":
        return _cmd_show(store, run_id=args.run_id, axes=args.axis)
    if args.cmd == "compare":
        return _cmd_compare(
            store, run_a=args.run_a, run_b=args.run_b, axes=args.axis,
        )
    parser.error(f"unknown subcommand {args.cmd!r}")
    return 2


# ---------- runs ----------


def _cmd_runs(store: ResultsStore, *, limit: int, recipe: str | None) -> int:
    rows = store.list_runs(limit=limit, recipe=recipe)
    if not rows:
        print("(no runs in store)" + (f" matching recipe={recipe!r}" if recipe else ""))
        return 0

    headers = ["ID", "RECIPE", "STARTED", "K", "TRIALS", "PASS^K"]
    table: list[list[str]] = [headers]
    for run in rows:
        passk = _overall_pass_pow_k(store, run)
        passk_str = f"{passk:.3f}" if passk is not None else "—"
        table.append(
            [
                str(run.id),
                _shorten(run.recipe, 40),
                _short_time(run.started_at),
                str(run.k),
                str(run.n_trials),
                passk_str,
            ]
        )
    _print_aligned_table(table)
    return 0


# ---------- show ----------


def _cmd_show(store: ResultsStore, *, run_id: int, axes: list[str]) -> int:
    run = store.get_run(run_id)
    if run is None:
        print(f"error: run {run_id} not found in {store.path}", file=sys.stderr)
        return 2

    trials = store.get_trials(run_id)
    if not trials:
        print(f"run {run_id}: (no trials recorded)")
        return 0

    print(f"run {run.id}")
    print(f"  recipe:     {run.recipe}")
    print(f"  k:          {run.k}")
    print(f"  trials:     {run.n_trials}")
    print(f"  started:    {run.started_at}")
    print(f"  finished:   {run.finished_at or '(in progress)'}")
    if run.notes:
        print(f"  notes:      {_shorten(run.notes, 80)}")

    trial_results = [_to_trial_result(t) for t in trials]
    overall_at_k, overall_pow_k = compute_passk([t.passed for t in trial_results], k=run.k)
    print()
    print(f"overall: pass@{run.k} = {overall_at_k:.3f}   pass^{run.k} = {overall_pow_k:.3f}")

    axes_to_show = axes or sorted({a for t in trials for a in t.axes.keys()})
    if not axes_to_show:
        print("\n(no axes recorded on these trials)")
        return 0
    for axis in axes_to_show:
        sliced = passk_by_slice(trial_results, axis, k=run.k)
        if not sliced:
            continue
        print(f"\nslice by {axis}:")
        for value, (at_k, pow_k) in sorted(sliced.items()):
            print(f"  {value:<20} pass@{run.k} = {at_k:.3f}   pass^{run.k} = {pow_k:.3f}")
    return 0


# ---------- compare ----------


def _cmd_compare(
    store: ResultsStore, *, run_a: int, run_b: int, axes: list[str],
) -> int:
    a = store.get_run(run_a)
    b = store.get_run(run_b)
    if a is None:
        print(f"error: run {run_a} not found", file=sys.stderr)
        return 2
    if b is None:
        print(f"error: run {run_b} not found", file=sys.stderr)
        return 2

    if a.recipe != b.recipe:
        print(
            f"warning: comparing runs from different recipes — "
            f"a={a.recipe!r}, b={b.recipe!r}",
            file=sys.stderr,
        )
    if a.k != b.k:
        print(
            f"warning: comparing runs with different k — a.k={a.k}, b.k={b.k}",
            file=sys.stderr,
        )

    trials_a = store.get_trials(a.id)
    trials_b = store.get_trials(b.id)
    if not trials_a or not trials_b:
        print("(one or both runs have no trials)", file=sys.stderr)
        return 2

    tr_a = [_to_trial_result(t) for t in trials_a]
    tr_b = [_to_trial_result(t) for t in trials_b]

    overall_a_atk, overall_a_powk = compute_passk([t.passed for t in tr_a], k=a.k)
    overall_b_atk, overall_b_powk = compute_passk([t.passed for t in tr_b], k=b.k)

    print(f"compare run {a.id} → run {b.id}")
    print(f"  recipe (a): {a.recipe}")
    print(f"  recipe (b): {b.recipe}")
    print()
    _print_overall_compare(a.k, b.k, overall_a_atk, overall_a_powk, overall_b_atk, overall_b_powk)

    axes_a = {ax for t in trials_a for ax in t.axes.keys()}
    axes_b = {ax for t in trials_b for ax in t.axes.keys()}
    if axes:
        axes_to_show = sorted(axes)
    else:
        axes_to_show = sorted(axes_a & axes_b)
    if not axes_to_show:
        print("\n(no shared axes between these runs; --axis to force)")
        return 0

    for axis in axes_to_show:
        sliced_a = passk_by_slice(tr_a, axis, k=a.k)
        sliced_b = passk_by_slice(tr_b, axis, k=b.k)
        all_values = sorted(set(sliced_a) | set(sliced_b))
        if not all_values:
            continue
        print(f"\nslice by {axis}:")
        for value in all_values:
            a_pair = sliced_a.get(value)
            b_pair = sliced_b.get(value)
            a_powk = a_pair[1] if a_pair else None
            b_powk = b_pair[1] if b_pair else None
            line = _format_compare_line(value, a_powk, b_powk, k=max(a.k, b.k))
            print(line)
    return 0


# ---------- helpers ----------


def _to_trial_result(t: TrialRow) -> TrialResult:
    return TrialResult(
        task_id=t.task_id,
        trial_index=t.trial_index,
        passed=t.passed,
        axes=dict(t.axes),
    )


def _overall_pass_pow_k(store: ResultsStore, run: RunRow) -> float | None:
    if run.n_trials == 0:
        return None
    trials = store.get_trials(run.id)
    if not trials:
        return None
    _, pow_k = compute_passk([t.passed for t in trials], k=run.k)
    return pow_k


def _short_time(iso: str | None) -> str:
    if not iso:
        return "—"
    # "2026-05-08T12:34:56.123456+00:00" -> "2026-05-08 12:34"
    short = iso.replace("T", " ")
    if "." in short:
        short = short.split(".", 1)[0]
    if "+" in short:
        short = short.rsplit("+", 1)[0]
    return short[:16]


def _shorten(s: str | None, n: int) -> str:
    if not s:
        return ""
    return s if len(s) <= n else s[: n - 1] + "…"


def _print_aligned_table(rows: Iterable[list[str]]) -> None:
    rows = list(rows)
    if not rows:
        return
    widths = [max(len(r[i]) for r in rows) for i in range(len(rows[0]))]
    for r in rows:
        print("  ".join(c.ljust(w) for c, w in zip(r, widths)).rstrip())


def _print_overall_compare(
    k_a: int, k_b: int,
    a_atk: float, a_powk: float,
    b_atk: float, b_powk: float,
) -> None:
    delta_atk = b_atk - a_atk
    delta_powk = b_powk - a_powk
    arrow = _trend_arrow(delta_powk)
    print(
        f"overall: pass@{k_a} a={a_atk:.3f}  pass@{k_b} b={b_atk:.3f}  Δ={delta_atk:+.3f}"
    )
    print(
        f"         pass^{k_a} a={a_powk:.3f}  pass^{k_b} b={b_powk:.3f}  "
        f"Δ={delta_powk:+.3f}  {arrow}"
    )


def _format_compare_line(
    value: str, a_powk: float | None, b_powk: float | None, *, k: int,
) -> str:
    if a_powk is None and b_powk is not None:
        return f"  {value:<20} (new in b)             pass^{k} b={b_powk:.3f}"
    if b_powk is None and a_powk is not None:
        return f"  {value:<20} pass^{k} a={a_powk:.3f}  (gone in b)"
    if a_powk is None and b_powk is None:
        return f"  {value:<20} (no data)"
    delta = b_powk - a_powk  # type: ignore[operator]
    return (
        f"  {value:<20} pass^{k} a={a_powk:.3f}  b={b_powk:.3f}  "
        f"Δ={delta:+.3f}  {_trend_arrow(delta)}"
    )


def _trend_arrow(delta: float) -> str:
    if delta > 0.005:
        return "↑"
    if delta < -0.005:
        return "↓"
    return "·"


if __name__ == "__main__":
    sys.exit(main())
