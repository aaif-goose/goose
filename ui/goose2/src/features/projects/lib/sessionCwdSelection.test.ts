import { beforeEach, describe, expect, it, vi } from "vitest";
import { resolvePath } from "@/shared/api/pathResolver";
import type { ProjectInfo } from "../api/projects";
import { resolveSessionCwd } from "./sessionCwdSelection";
import {
  defaultGlobalArtifactRoot,
  resolveProjectDefaultArtifactRoot,
} from "./chatProjectContext";

vi.mock("@/shared/api/pathResolver", () => ({
  resolvePath: vi.fn(),
}));

function makeProject(overrides: Partial<ProjectInfo> = {}): ProjectInfo {
  return {
    id: "project-1",
    path: "/tmp/projects/project-1.md",
    name: "Project",
    description: "",
    prompt: "",
    icon: "folder",
    color: "#000000",
    preferredProvider: null,
    preferredModel: null,
    workingDirs: [],
    useWorktrees: false,
    order: 0,
    archivedAt: null,
    ...overrides,
  };
}

describe("sessionCwdSelection", () => {
  beforeEach(() => {
    vi.mocked(resolvePath).mockReset();
  });

  it("resolves the first workspace root to the default artifact root", () => {
    expect(
      resolveProjectDefaultArtifactRoot(
        makeProject({
          workingDirs: ["/Users/wesb/dev/goose2", "/Users/wesb/dev/other"],
        }),
      ),
    ).toBe("/Users/wesb/dev/goose2/artifacts");
  });

  it("returns undefined when no workspace roots exist", () => {
    expect(
      resolveProjectDefaultArtifactRoot(makeProject({ workingDirs: [] })),
    ).toBeUndefined();
  });

  it("falls back to global artifacts for a project without working dirs", async () => {
    vi.mocked(resolvePath).mockResolvedValue({
      path: "/Users/wesb/.goose/artifacts",
    });

    await expect(
      resolveSessionCwd(makeProject({ workingDirs: [] })),
    ).resolves.toBe("/Users/wesb/.goose/artifacts");

    expect(resolvePath).toHaveBeenCalledWith({
      parts: ["~", ".goose", "artifacts"],
    });
  });

  describe("defaultGlobalArtifactRoot", () => {
    it("resolves the global artifact root through the path resolver", async () => {
      vi.mocked(resolvePath).mockResolvedValue({
        path: "/Users/wesb/.goose/artifacts",
      });

      await expect(defaultGlobalArtifactRoot()).resolves.toBe(
        "/Users/wesb/.goose/artifacts",
      );

      expect(resolvePath).toHaveBeenCalledWith({
        parts: ["~", ".goose", "artifacts"],
      });
    });
  });
});
