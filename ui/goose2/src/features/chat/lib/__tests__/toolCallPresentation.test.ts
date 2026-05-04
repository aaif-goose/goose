import { describe, expect, it } from "vitest";
import {
  getToolInputSummaryRows,
  isHoistableText,
  isStringifiedCopyOfStructured,
} from "../toolCallPresentation";

describe("getToolInputSummaryRows", () => {
  it("returns Command + Working directory rows for shell-style args", () => {
    const rows = getToolInputSummaryRows({
      name: "developer__shell",
      arguments: { command: "npm test", cwd: "/repo" },
    });
    expect(rows).toEqual([
      {
        label: "Command",
        value: "npm test",
        monospace: true,
        renderAs: "bash",
      },
      { label: "Working directory", value: "/repo", monospace: true },
    ]);
  });

  it("returns a Resource row for url args", () => {
    const rows = getToolInputSummaryRows({
      name: "fetch",
      arguments: { url: "https://example.com" },
    });
    expect(rows).toEqual([
      { label: "Resource", value: "https://example.com", monospace: true },
    ]);
  });

  it("returns Query + Path rows for search-style args", () => {
    const rows = getToolInputSummaryRows({
      name: "developer__grep",
      arguments: { query: "TODO", path: "/repo/src" },
    });
    expect(rows).toEqual([
      { label: "Query", value: "TODO", monospace: true },
      { label: "Path", value: "/repo/src", monospace: true },
    ]);
  });

  it("collapses long file paths to basenames and preserves the full path in title", () => {
    const rows = getToolInputSummaryRows({
      name: "developer__edit",
      arguments: { path: "/Users/tho/repo/src/lib/index.ts" },
    });
    expect(rows).toEqual([
      {
        label: "Path",
        value: "index.ts",
        monospace: true,
        title: "/Users/tho/repo/src/lib/index.ts",
      },
    ]);
  });

  it("includes Line when present alongside a path", () => {
    const rows = getToolInputSummaryRows({
      name: "developer__read",
      arguments: { path: "/repo/foo.ts", line: 42 },
    });
    expect(rows).toEqual([
      {
        label: "Path",
        value: "foo.ts",
        monospace: true,
        title: "/repo/foo.ts",
      },
      { label: "Line", value: "42" },
    ]);
  });

  it("falls back to Tool name when no familiar arg keys are present", () => {
    const rows = getToolInputSummaryRows({
      name: "custom-extension",
      arguments: { foo: 1 },
    });
    expect(rows).toEqual([{ label: "Tool", value: "custom-extension" }]);
  });

  it("returns an empty list when args and name are both empty", () => {
    expect(getToolInputSummaryRows({ name: "", arguments: {} })).toEqual([]);
  });

  it("ignores empty string values when scanning args", () => {
    const rows = getToolInputSummaryRows({
      name: "developer__shell",
      arguments: { command: "   ", cwd: "/repo" },
    });
    expect(rows).toEqual([
      {
        label: "Path",
        value: "repo",
        monospace: true,
        title: "/repo",
      },
    ]);
  });
});

describe("isHoistableText", () => {
  it("accepts a short single-line string", () => {
    expect(isHoistableText("Found 3 matches")).toBe(true);
  });

  it("rejects multi-line strings", () => {
    expect(isHoistableText("line one\nline two")).toBe(false);
    expect(isHoistableText("line one\rline two")).toBe(false);
  });

  it("rejects empty / whitespace-only strings", () => {
    expect(isHoistableText("")).toBe(false);
    expect(isHoistableText("   ")).toBe(false);
    expect(isHoistableText(undefined)).toBe(false);
  });

  it("rejects strings longer than the max length budget", () => {
    expect(isHoistableText("x".repeat(80))).toBe(true);
    expect(isHoistableText("x".repeat(81))).toBe(false);
  });

  it("trims before measuring length and line count", () => {
    expect(isHoistableText("  Found 3 matches  ")).toBe(true);
    expect(isHoistableText(`  ${"x".repeat(81)}  `)).toBe(false);
  });
});

describe("isStringifiedCopyOfStructured", () => {
  it("returns true when text is a compact JSON stringification of structured", () => {
    const structured = { kind: "summary", count: 3 };
    expect(
      isStringifiedCopyOfStructured(JSON.stringify(structured), structured),
    ).toBe(true);
  });

  it("returns true when text is a pretty-printed JSON stringification", () => {
    const structured = { kind: "summary", count: 3 };
    expect(
      isStringifiedCopyOfStructured(
        JSON.stringify(structured, null, 2),
        structured,
      ),
    ).toBe(true);
  });

  it("returns false when text is not valid JSON", () => {
    expect(isStringifiedCopyOfStructured("Found 3 matches", { count: 3 })).toBe(
      false,
    );
  });

  it("returns false when parsed text differs structurally from structured", () => {
    expect(
      isStringifiedCopyOfStructured(JSON.stringify({ count: 3 }), { count: 4 }),
    ).toBe(false);
  });

  it("returns false when either side is missing", () => {
    expect(isStringifiedCopyOfStructured(undefined, { count: 3 })).toBe(false);
    expect(isStringifiedCopyOfStructured("{}", undefined)).toBe(false);
  });

  it("treats null structured as a valid comparison target", () => {
    expect(isStringifiedCopyOfStructured("null", null)).toBe(true);
  });
});
