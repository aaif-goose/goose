import { describe, expect, it } from "vitest";
import { advanceVadState, createInitialVadState } from "./dictationVad";

function runFrames(levels: number[]) {
  const decisions: string[] = [];
  let state = createInitialVadState();

  for (const level of levels) {
    const result = advanceVadState(state, level);
    decisions.push(result.decision);
    state = result.nextState;
  }

  return decisions;
}

describe("dictationVad", () => {
  it("ignores silence-only audio", () => {
    expect(runFrames([0, 0, 0, 0])).toEqual([
      "ignore",
      "ignore",
      "ignore",
      "ignore",
    ]);
  });

  it("discards short noise bursts that never confirm speech", () => {
    expect(runFrames([0.03, 0, 0, 0])).toEqual([
      "append",
      "append",
      "append",
      "discard",
    ]);
  });

  it("flushes a chunk after speech followed by trailing silence", () => {
    expect(runFrames([0.03, 0.03, 0.03, 0, 0, 0, 0, 0, 0])).toContain(
      "append_and_flush",
    );
  });

  it("returns to ignoring silence after a flush, ready for another chunk", () => {
    const decisions = runFrames([
      0.03, 0.03, 0.03, 0, 0, 0, 0, 0, 0, 0.03, 0.03, 0.03, 0, 0, 0, 0, 0, 0,
    ]);

    expect(
      decisions.filter((decision) => decision === "append_and_flush"),
    ).toHaveLength(2);
  });
});
