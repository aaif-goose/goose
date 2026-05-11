import { describe, expect, it } from "vitest";
import { extractToolStructuredContent } from "../acpToolCallContent";

describe("extractToolStructuredContent", () => {
  it.each([
    [{ restaurants: [{ name: "Coffee Shop" }] }, "object"],
    ["complete", "string"],
    [42, "number"],
    [false, "boolean"],
    [null, "null"],
  ])("preserves %s rawOutput values", (rawOutput, _label) => {
    expect(extractToolStructuredContent({ rawOutput })).toEqual(rawOutput);
  });

  it("returns undefined when rawOutput is absent", () => {
    expect(extractToolStructuredContent({})).toBeUndefined();
  });
});
