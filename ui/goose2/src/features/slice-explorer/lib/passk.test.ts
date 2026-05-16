import { describe, expect, it } from "vitest";
import { computePassk, passkByAxis, recordedAxes, sliceByAxis } from "./passk";
import type { Trial } from "../types";

function trial(
  taskId: string,
  trialIndex: number,
  passed: boolean,
  axes: Trial["axes"] = {},
): Trial {
  return {
    taskId,
    trialIndex,
    passed,
    polarity: "positive",
    tags: [],
    axes,
    durationMs: null,
  };
}

describe("computePassk", () => {
  it("computes pass@k and pass^k from observed pass rate", () => {
    // p = 3/4, k = 2 → pass@k = 1 - 0.25^2 = 0.9375; pass^k = 0.75^2 = 0.5625
    const { passAtK, passPowK } = computePassk([true, true, true, false], 2);
    expect(passAtK).toBeCloseTo(0.9375);
    expect(passPowK).toBeCloseTo(0.5625);
  });

  it("returns 1.0 for all-pass at any k <= n", () => {
    const { passAtK, passPowK } = computePassk([true, true, true], 3);
    expect(passAtK).toBeCloseTo(1.0);
    expect(passPowK).toBeCloseTo(1.0);
  });

  it("returns 0.0 for all-fail", () => {
    const { passAtK, passPowK } = computePassk([false, false], 2);
    expect(passAtK).toBeCloseTo(0.0);
    expect(passPowK).toBeCloseTo(0.0);
  });

  it("throws on empty input", () => {
    expect(() => computePassk([], 1)).toThrow();
  });

  it("throws when k exceeds observed trials", () => {
    expect(() => computePassk([true, false], 3)).toThrow(/extrapolate/);
  });
});

describe("sliceByAxis", () => {
  it("groups trials by axis value", () => {
    const trials = [
      trial("t1", 0, true, { complexity: "low" }),
      trial("t1", 1, false, { complexity: "low" }),
      trial("t2", 0, true, { complexity: "high" }),
    ];
    const grouped = sliceByAxis(trials, "complexity");
    expect(grouped.get("low")?.length).toBe(2);
    expect(grouped.get("high")?.length).toBe(1);
  });

  it("buckets missing-axis trials into '<unset>' rather than dropping them", () => {
    const trials = [
      trial("t1", 0, true, { complexity: "low" }),
      trial("t2", 0, false, {}),
    ];
    const grouped = sliceByAxis(trials, "complexity");
    expect(grouped.get("<unset>")?.length).toBe(1);
    expect(grouped.get("low")?.length).toBe(1);
  });
});

describe("passkByAxis", () => {
  it("returns sorted cells per axis value with correct math", () => {
    const trials = [
      trial("t1", 0, true, { complexity: "low" }),
      trial("t1", 1, true, { complexity: "low" }),
      trial("t2", 0, false, { complexity: "high" }),
      trial("t2", 1, false, { complexity: "high" }),
    ];
    const cells = passkByAxis(trials, "complexity", 2);
    expect(cells.map((c) => c.value)).toEqual(["high", "low"]);
    expect(cells[0].passPowK).toBeCloseTo(0.0);
    expect(cells[1].passPowK).toBeCloseTo(1.0);
  });

  it("clamps k to the bucket size when smaller than requested k", () => {
    const trials = [trial("t1", 0, true, { kind: "a" })];
    const cells = passkByAxis(trials, "kind", 5);
    expect(cells[0].passPowK).toBeCloseTo(1.0);
  });
});

describe("recordedAxes", () => {
  it("returns the union of axis names across all trials, sorted", () => {
    const trials = [
      trial("t1", 0, true, { a: 1, b: 2 }),
      trial("t2", 0, false, { b: 3, c: 4 }),
    ];
    expect(recordedAxes(trials)).toEqual(["a", "b", "c"]);
  });
});
