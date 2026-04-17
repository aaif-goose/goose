import { describe, expect, it } from "vitest";
import {
  buildSessionCwdParts,
  defaultGlobalArtifactRoot,
  resolveDefaultSessionCwd,
  resolveProjectDefaultArtifactRoot,
} from "./sessionCwdSelection";

describe("sessionCwdSelection", () => {
  it("resolves the first workspace root to the default artifact root", () => {
    expect(
      resolveProjectDefaultArtifactRoot({
        workingDirs: ["/Users/wesb/dev/goose2", "/Users/wesb/dev/other"],
        artifactsDir: "/Users/wesb/.goose/projects/goose2/artifacts",
      }),
    ).toBe("/Users/wesb/dev/goose2/artifacts");
  });

  it("falls back to the stored project artifact root when no workspace roots exist", () => {
    expect(
      resolveProjectDefaultArtifactRoot({
        workingDirs: [],
        artifactsDir: "/Users/wesb/.goose/projects/sample-project/artifacts",
      }),
    ).toBe("/Users/wesb/.goose/projects/sample-project/artifacts");
  });

  describe("defaultGlobalArtifactRoot", () => {
    it("normalises path separators and appends .goose/artifacts", () => {
      expect(defaultGlobalArtifactRoot("/Users/wesb")).toBe(
        "/Users/wesb/.goose/artifacts",
      );
    });

    it("normalises backslashes on Windows-style paths", () => {
      expect(defaultGlobalArtifactRoot("C:\\Users\\wesb\\")).toBe(
        "C:/Users/wesb/.goose/artifacts",
      );
    });

    it("strips trailing slashes", () => {
      expect(defaultGlobalArtifactRoot("/Users/wesb/")).toBe(
        "/Users/wesb/.goose/artifacts",
      );
    });
  });

  describe("resolveDefaultSessionCwd", () => {
    it("returns the project default artifact root without requiring homeDir", () => {
      expect(
        resolveDefaultSessionCwd({
          workingDirs: ["/Users/wesb/dev/goose2"],
          artifactsDir: "/Users/wesb/.goose/projects/goose2/artifacts",
        }),
      ).toBe("/Users/wesb/dev/goose2/artifacts");
    });

    it("returns the project default artifact root when available", () => {
      expect(
        resolveDefaultSessionCwd(
          {
            workingDirs: ["/Users/wesb/dev/goose2"],
            artifactsDir: "/Users/wesb/.goose/projects/goose2/artifacts",
          },
          "/Users/wesb",
        ),
      ).toBe("/Users/wesb/dev/goose2/artifacts");
    });

    it("returns undefined when a project exists but has no working dirs", () => {
      expect(
        resolveDefaultSessionCwd(
          { workingDirs: [], artifactsDir: "" },
          "/Users/wesb",
        ),
      ).toBeUndefined();
    });

    it("falls back to home artifacts dir when no project", () => {
      expect(resolveDefaultSessionCwd(null, "/Users/wesb")).toBe(
        "/Users/wesb/.goose/artifacts",
      );
    });

    it("does not resolve a non-project fallback without homeDir", () => {
      expect(resolveDefaultSessionCwd(null)).toBeUndefined();
    });
  });

  describe("buildSessionCwdParts", () => {
    it("prefers the active workspace path when present", () => {
      expect(
        buildSessionCwdParts(
          {
            workingDirs: ["/Users/wesb/dev/goose2"],
            artifactsDir: "/Users/wesb/.goose/projects/goose2/artifacts",
          },
          "/Users/wesb/dev/goose2-worktree",
        ),
      ).toEqual(["/Users/wesb/dev/goose2-worktree"]);
    });

    it("uses the first project workspace root plus artifacts for project defaults", () => {
      expect(
        buildSessionCwdParts({
          workingDirs: ["/Users/wesb/dev/goose2"],
          artifactsDir: "/Users/wesb/.goose/projects/goose2/artifacts",
        }),
      ).toEqual(["/Users/wesb/dev/goose2", "artifacts"]);
    });

    it("falls back to the stored project artifacts root when no workspace roots exist", () => {
      expect(
        buildSessionCwdParts({
          workingDirs: [],
          artifactsDir: "/Users/wesb/.goose/projects/sample-project/artifacts",
        }),
      ).toEqual(["/Users/wesb/.goose/projects/sample-project/artifacts"]);
    });

    it("returns the global fallback parts when no project exists", () => {
      expect(buildSessionCwdParts(null)).toEqual(["~", ".goose", "artifacts"]);
    });

    it("returns undefined for a project with no usable runtime path", () => {
      expect(
        buildSessionCwdParts({ workingDirs: [], artifactsDir: "   " }),
      ).toBeUndefined();
    });
  });
});
