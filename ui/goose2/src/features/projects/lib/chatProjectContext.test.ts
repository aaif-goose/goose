import { describe, expect, it } from "vitest";
import {
  composeSystemPrompt,
  getProjectArtifactRoots,
  getProjectFolderName,
  getProjectFolderOption,
} from "./chatProjectContext";

describe("chatProjectContext", () => {
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
