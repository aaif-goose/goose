import { describe, expect, it } from "vitest";
import {
  buildProjectSystemPrompt,
  composeSystemPrompt,
  getProjectArtifactRoots,
  getProjectFolderName,
  getProjectFolderOption,
} from "./chatProjectContext";

describe("chatProjectContext", () => {
  it("builds project instructions from stored project settings", () => {
    const systemPrompt = buildProjectSystemPrompt({
      id: "project-1",
      name: "Goose2",
      description: "Desktop app",
      prompt: "Always read AGENTS.md before editing.",
      icon: "folder",
      color: "#000000",
      preferredProvider: "goose",
      preferredModel: "claude-sonnet-4",
      workingDirs: ["/Users/wesb/dev/goose2"],
      useWorktrees: true,
      order: 0,
      archivedAt: null,
      createdAt: "now",
      updatedAt: "now",
    });

    expect(systemPrompt).toContain("<project-settings>");
    expect(systemPrompt).toContain("Project name: Goose2");
    expect(systemPrompt).toContain(
      "Working directories: /Users/wesb/dev/goose2",
    );
    expect(systemPrompt).toContain(
      "Default working directory: /Users/wesb/dev/goose2",
    );
    expect(systemPrompt).toContain("Preferred provider: goose");
    expect(systemPrompt).toContain(
      "Use git worktrees for branch isolation: yes",
    );
    expect(systemPrompt).toContain("<project-file-policy>");
    expect(systemPrompt).toContain(
      "Use /Users/wesb/dev/goose2 as the default working directory for this project.",
    );
    expect(systemPrompt).toContain("<project-instructions>");
    expect(systemPrompt).toContain("Always read AGENTS.md before editing.");
  });

  it("combines persona and project prompts without empty sections", () => {
    expect(
      composeSystemPrompt("Persona prompt", undefined, "Project prompt"),
    ).toBe("Persona prompt\n\nProject prompt");
  });

  it("extracts the folder name from a path", () => {
    expect(getProjectFolderName("/Users/wesb/dev/goose2")).toBe("goose2");
    expect(getProjectFolderName("C:\\Users\\wesb\\goose2\\")).toBe("goose2");
  });

  it("creates folder options from the project's working directories", () => {
    expect(
      getProjectFolderOption({
        workingDirs: ["/Users/wesb/dev/goose2", "/Users/wesb/dev/other"],
      }),
    ).toEqual([
      {
        id: "/Users/wesb/dev/goose2",
        name: "goose2",
        path: "/Users/wesb/dev/goose2",
      },
      {
        id: "/Users/wesb/dev/other",
        name: "other",
        path: "/Users/wesb/dev/other",
      },
    ]);
  });

  it("returns an empty array when workingDirs is empty", () => {
    expect(
      getProjectFolderOption({
        workingDirs: [],
      }),
    ).toEqual([]);
  });

  it("returns an empty array when project is null", () => {
    expect(getProjectFolderOption(null)).toEqual([]);
  });

  it("returns working dirs unchanged", () => {
    expect(
      getProjectArtifactRoots({
        workingDirs: ["/Users/wesb/dev/goose2", "/Users/wesb/dev/other"],
      }),
    ).toEqual(["/Users/wesb/dev/goose2", "/Users/wesb/dev/other"]);
  });
});
