import { describe, expect, it } from "vitest";
import { defaultExportFilename } from "./exportSession";

describe("defaultExportFilename", () => {
  it("builds a safe default export filename from the session title", () => {
    expect(defaultExportFilename("  Foo:/Bar*Baz  ")).toBe("Foo--Bar-Baz.json");
    expect(defaultExportFilename("")).toBe("session.json");
  });
});
