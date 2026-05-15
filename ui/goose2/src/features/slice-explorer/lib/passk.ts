import type { Trial } from "../types";

/**
 * pass@k = 1 - (1 - p)^k where p is the observed pass rate.
 * pass^k = p^k.
 *
 * Mirrors `eval-bench/lib/kpass.py::compute_passk`. Trials must be
 * non-empty; `k` must not exceed observed trial count (no extrapolation).
 */
export function computePassk(
  passed: boolean[],
  k: number,
): { passAtK: number; passPowK: number } {
  if (passed.length === 0) {
    throw new Error("computePassk requires at least one trial");
  }
  if (k > passed.length) {
    throw new Error(
      `k=${k} exceeds observed trial count ${passed.length}; cannot extrapolate`,
    );
  }
  const passes = passed.reduce((sum, t) => sum + (t ? 1 : 0), 0);
  const p = passes / passed.length;
  return {
    passAtK: 1 - (1 - p) ** k,
    passPowK: p ** k,
  };
}

/**
 * Group trials by the value of one axis. Trials missing the axis go into
 * an explicit "<unset>" bucket — never silently dropped, matching the
 * Python implementation.
 */
export function sliceByAxis(
  trials: Trial[],
  axis: string,
): Map<string, Trial[]> {
  const out = new Map<string, Trial[]>();
  for (const trial of trials) {
    const raw = trial.axes[axis];
    const key = raw == null ? "<unset>" : String(raw);
    const bucket = out.get(key);
    if (bucket) {
      bucket.push(trial);
    } else {
      out.set(key, [trial]);
    }
  }
  return out;
}

export interface SliceCell {
  value: string;
  passAtK: number;
  passPowK: number;
  nTrials: number;
}

export function passkByAxis(
  trials: Trial[],
  axis: string,
  k: number,
): SliceCell[] {
  const sliced = sliceByAxis(trials, axis);
  const out: SliceCell[] = [];
  for (const [value, bucket] of sliced.entries()) {
    if (bucket.length === 0) continue;
    const { passAtK, passPowK } = computePassk(
      bucket.map((t) => t.passed),
      Math.min(k, bucket.length),
    );
    out.push({
      value,
      passAtK,
      passPowK,
      nTrials: bucket.length,
    });
  }
  return out.sort((a, b) => a.value.localeCompare(b.value));
}

/** Discover every axis name recorded on at least one trial in the run. */
export function recordedAxes(trials: Trial[]): string[] {
  const names = new Set<string>();
  for (const trial of trials) {
    for (const axis of Object.keys(trial.axes)) {
      names.add(axis);
    }
  }
  return Array.from(names).sort();
}
