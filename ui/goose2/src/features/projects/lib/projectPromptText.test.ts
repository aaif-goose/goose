import { describe, expect, it } from "vitest";
import {
  buildEditorText,
  hasEquivalentWorkingDir,
  insertWorkingDir,
  parseEditorText,
} from "./projectPromptText";

describe("projectPromptText", () => {
  it("round-trips working directories and prompt text", () => {
    const text = buildEditorText(
      ["/tmp/one", "/tmp/two"],
      "Follow AGENTS.md\nThen fix the issue",
    );

    expect(parseEditorText(text)).toEqual({
      prompt: "Follow AGENTS.md\nThen fix the issue",
      workingDirs: ["/tmp/one", "/tmp/two"],
    });
  });

  it("places includes after the prompt text", () => {
    expect(buildEditorText(["/tmp/one"], "My prompt")).toBe(
      "My prompt\n\ninclude: /tmp/one",
    );
  });

  it("handles includes-only text (no prompt)", () => {
    expect(buildEditorText(["/tmp/one", "/tmp/two"], "")).toBe(
      "include: /tmp/one\ninclude: /tmp/two",
    );
  });

  it("collects include lines from anywhere in the editor text", () => {
    expect(
      parseEditorText("include: /tmp/one\nprompt\n\ninclude: /tmp/two"),
    ).toEqual({
      prompt: "prompt",
      workingDirs: ["/tmp/one", "/tmp/two"],
    });
  });

  it("treats trailing include lines as working directories when saving", () => {
    expect(parseEditorText("prompt first\ninclude: /tmp/kept")).toEqual({
      prompt: "prompt first",
      workingDirs: ["/tmp/kept"],
    });
  });

  it("creates a trailing include block for prompt-only text", () => {
    expect(insertWorkingDir("Existing prompt", "/tmp/one")).toBe(
      "Existing prompt\n\ninclude: /tmp/one",
    );
  });

  it("appends a directory to the trailing include block", () => {
    expect(
      insertWorkingDir("Prompt body\n\ninclude: /tmp/one", "/tmp/two"),
    ).toBe("Prompt body\n\ninclude: /tmp/one\ninclude: /tmp/two");
  });

  it("adds a new directory to the bottom without moving existing prompt text", () => {
    expect(insertWorkingDir("include: /tmp/one\nPrompt body", "/tmp/two")).toBe(
      "include: /tmp/one\nPrompt body\n\ninclude: /tmp/two",
    );
  });

  it("treats tilde and absolute paths as equivalent when checking duplicates", () => {
    expect(
      hasEquivalentWorkingDir(
        "Prompt body\ninclude: ~/dev/goose2",
        "/Users/mtoohey/dev/goose2",
        "/Users/mtoohey",
      ),
    ).toBe(true);
  });

  it("does not match different directories when checking duplicates", () => {
    expect(
      hasEquivalentWorkingDir(
        "Prompt body\ninclude: ~/dev/goose2",
        "/Users/mtoohey/dev/other",
        "/Users/mtoohey",
      ),
    ).toBe(false);
  });

  it("treats trailing slashes as equivalent when checking duplicates", () => {
    expect(
      hasEquivalentWorkingDir(
        "Prompt body\ninclude: /Users/mtoohey/dev/goose2/",
        "/Users/mtoohey/dev/goose2",
        "/Users/mtoohey",
      ),
    ).toBe(true);
  });
});
