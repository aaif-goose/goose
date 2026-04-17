import { beforeEach, describe, expect, it, vi } from "vitest";
import { resolvePath } from "@/shared/api/pathResolver";
import { resolveSessionCwd } from "./sessionCwdSelection";
import {
  defaultGlobalArtifactRoot,
  resolveProjectDefaultArtifactRoot,
} from "./chatProjectContext";

vi.mock("@/shared/api/pathResolver", () => ({
  resolvePath: vi.fn(),
}));

describe("sessionCwdSelection", () => {
  beforeEach(() => {
    vi.mocked(resolvePath).mockReset();
  });

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

  it("returns undefined for a pathless project artifact root", () => {
    expect(
      resolveProjectDefaultArtifactRoot({
        workingDirs: [],
        artifactsDir: "   ",
      }),
    ).toBeUndefined();
  });

  it("falls back to global artifacts for a pathless project session cwd", async () => {
    vi.mocked(resolvePath).mockResolvedValue({
      path: "/Users/wesb/.goose/artifacts",
    });

    await expect(
      resolveSessionCwd({
        workingDirs: [],
        artifactsDir: "   ",
      }),
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
