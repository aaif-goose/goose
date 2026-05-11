import { describe, expect, it } from "vitest";
import { summarizeToolChainSteps } from "../toolChainSummary";
import type { ToolChainItem } from "../toolChainGrouping";

let nextId = 0;

function step(name: string): ToolChainItem {
  const id = `step-${++nextId}`;
  return {
    key: id,
    request: {
      type: "toolRequest",
      id,
      name,
      arguments: {},
      status: "completed",
    },
  };
}

describe("summarizeToolChainSteps", () => {
  it("classifies file reads as reviewing_files", () => {
    const summary = summarizeToolChainSteps([
      step("Read · src/index.ts"),
      step("List · src/"),
    ]);
    expect(summary.kind).toBe("reviewing_files");
    expect(summary.titleKey).toBe("tool_chain.summary.reviewing_files");
    expect(summary.count).toBe(2);
  });

  it("classifies shell/bash steps as running_commands", () => {
    const summary = summarizeToolChainSteps([
      step("Shell · ls -la"),
      step("Shell · cargo test"),
      step("Read · Cargo.toml"),
    ]);
    expect(summary.kind).toBe("running_commands");
    expect(summary.titleKey).toBe("tool_chain.summary.running_commands");
  });

  it("classifies write/edit-heavy chains as updating_files", () => {
    const summary = summarizeToolChainSteps([
      step("Edit · src/lib.rs"),
      step("Write · src/new.rs"),
      step("Read · src/lib.rs"),
    ]);
    expect(summary.kind).toBe("updating_files");
    expect(summary.titleKey).toBe("tool_chain.summary.updating_files");
  });

  it("classifies fetch/url steps as checking_resources", () => {
    const summary = summarizeToolChainSteps([
      step("Fetch · https://example.com"),
      step("Fetch · https://example.com/api"),
    ]);
    expect(summary.kind).toBe("checking_resources");
    expect(summary.titleKey).toBe("tool_chain.summary.checking_resources");
  });

  it("uses the detail to detect URLs even when the prefix is generic", () => {
    const summary = summarizeToolChainSteps([
      step("Tool · https://example.com"),
      step("Tool · https://example.com/api"),
    ]);
    expect(summary.kind).toBe("checking_resources");
  });

  it("falls back to reviewing_files when nothing dominates", () => {
    const summary = summarizeToolChainSteps([
      step("Read · src/a.ts"),
      step("Read · src/b.ts"),
      step("Read · src/c.ts"),
    ]);
    expect(summary.kind).toBe("reviewing_files");
  });

  it("returns a sane default for empty chains", () => {
    const summary = summarizeToolChainSteps([]);
    expect(summary.count).toBe(0);
    expect(summary.kind).toBe("reviewing_files");
  });

  it("breaks ties between updating and reviewing in favor of updating only when strictly greater", () => {
    const summary = summarizeToolChainSteps([
      step("Edit · src/a.ts"),
      step("Read · src/b.ts"),
    ]);
    expect(summary.kind).toBe("reviewing_files");
  });

  it("treats command tokens correctly when prefix contains 'execute'", () => {
    const summary = summarizeToolChainSteps([
      step("Execute · npm test"),
      step("Execute · npm build"),
    ]);
    expect(summary.kind).toBe("running_commands");
  });
});
