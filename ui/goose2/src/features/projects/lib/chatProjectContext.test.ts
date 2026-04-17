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
      artifactsDir: "/Users/wesb/.goose/projects/goose2/artifacts",
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
      "Artifact directory: /Users/wesb/dev/goose2/artifacts",
    );
    expect(systemPrompt).toContain("Preferred provider: goose");
    expect(systemPrompt).toContain(
      "Use git worktrees for branch isolation: yes",
    );
    expect(systemPrompt).toContain("<project-file-policy>");
    expect(systemPrompt).toContain(
      "Write newly generated files to /Users/wesb/dev/goose2/artifacts by default.",
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
        artifactsDir: "/Users/wesb/.goose/projects/goose2/artifacts",
      }),
    ).toEqual([
      {
        id: "/Users/wesb/dev/goose2/artifacts",
        name: "artifacts",
        path: "/Users/wesb/dev/goose2/artifacts",
      },
      {
        id: "/Users/wesb/dev/other/artifacts",
        name: "artifacts",
        path: "/Users/wesb/dev/other/artifacts",
      },
    ]);
  });

  it("returns an empty array when workingDirs is empty", () => {
    expect(
      getProjectFolderOption({
        workingDirs: [],
        artifactsDir: "/Users/wesb/.goose/projects/sample-project/artifacts",
      }),
    ).toEqual([
      {
        id: "/Users/wesb/.goose/projects/sample-project/artifacts",
        name: "artifacts",
        path: "/Users/wesb/.goose/projects/sample-project/artifacts",
      },
    ]);
  });

  it("returns an empty array when project is null", () => {
    expect(getProjectFolderOption(null)).toEqual([]);
  });

  it("returns only artifact subdirectories for working dirs", () => {
    expect(
      getProjectArtifactRoots({
        workingDirs: ["/Users/wesb/dev/goose2", "/Users/wesb/dev/other"],
        artifactsDir: "/Users/wesb/.goose/projects/goose2/artifacts",
      }),
    ).toEqual([
      "/Users/wesb/dev/goose2/artifacts",
      "/Users/wesb/dev/other/artifacts",
    ]);
  });
});
