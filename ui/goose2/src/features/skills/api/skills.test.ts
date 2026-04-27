import { beforeEach, describe, expect, it, vi } from "vitest";

const mockGooseSourcesList = vi.fn();

vi.mock("@/shared/api/acpConnection", () => ({
  getClient: async () => ({
    goose: {
      GooseSourcesList: (...args: unknown[]) => mockGooseSourcesList(...args),
    },
  }),
}));

describe("listSkills", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.resetModules();
  });

  it("aggregates project skill listings and recognizes .agents skill paths", async () => {
    mockGooseSourcesList
      .mockResolvedValueOnce({
        sources: [
          {
            type: "skill",
            name: "code-review",
            description: "Reviews code",
            content: "Review carefully",
            directory: "/Users/test/.agents/skills/code-review",
            global: true,
          },
        ],
      })
      .mockResolvedValueOnce({
        sources: [
          {
            type: "skill",
            name: "code-review",
            description: "Reviews code",
            content: "Review carefully",
            directory: "/Users/test/.agents/skills/code-review",
            global: true,
          },
          {
            type: "skill",
            name: "test-writer",
            description: "Writes tests",
            content: "Write tests",
            directory: "/tmp/alpha/.agents/skills/test-writer",
            global: false,
          },
        ],
      });

    const { listSkills } = await import("./skills");
    const skills = await listSkills(["/tmp/alpha", "/tmp/alpha"]);

    expect(mockGooseSourcesList).toHaveBeenNthCalledWith(1, {
      type: "skill",
    });
    expect(mockGooseSourcesList).toHaveBeenNthCalledWith(2, {
      type: "skill",
      projectDir: "/tmp/alpha",
    });
    expect(skills.filter((skill) => skill.name === "code-review")).toHaveLength(
      1,
    );
    expect(skills).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          name: "test-writer",
          sourceKind: "project",
          sourceLabel: "alpha",
          projectLinks: [
            {
              id: "/tmp/alpha",
              name: "alpha",
              workingDir: "/tmp/alpha",
            },
          ],
        }),
      ]),
    );
  });

  it("recognizes legacy .goose project skill paths", async () => {
    mockGooseSourcesList
      .mockResolvedValueOnce({ sources: [] })
      .mockResolvedValueOnce({
        sources: [
          {
            type: "skill",
            name: "legacy-writer",
            description: "Legacy project skill",
            content: "Legacy instructions",
            directory: "/tmp/beta/.goose/skills/legacy-writer",
            global: false,
          },
        ],
      });

    const { listSkills } = await import("./skills");
    const skills = await listSkills(["/tmp/beta"]);

    expect(skills).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          name: "legacy-writer",
          sourceKind: "project",
          sourceLabel: "beta",
          projectLinks: [
            {
              id: "/tmp/beta",
              name: "beta",
              workingDir: "/tmp/beta",
            },
          ],
        }),
      ]),
    );
  });

  it("keeps available skills when a project skill listing fails", async () => {
    mockGooseSourcesList
      .mockResolvedValueOnce({
        sources: [
          {
            type: "skill",
            name: "code-review",
            description: "Reviews code",
            content: "Review carefully",
            directory: "/Users/test/.agents/skills/code-review",
            global: true,
          },
        ],
      })
      .mockRejectedValueOnce(new Error("permission denied"))
      .mockResolvedValueOnce({
        sources: [
          {
            type: "skill",
            name: "test-writer",
            description: "Writes tests",
            content: "Write tests",
            directory: "/tmp/beta/.agents/skills/test-writer",
            global: false,
          },
        ],
      });

    const { listSkills } = await import("./skills");
    const skills = await listSkills(["/tmp/alpha", "/tmp/beta"]);

    expect(mockGooseSourcesList).toHaveBeenCalledTimes(3);
    expect(skills.map((skill) => skill.name)).toEqual([
      "code-review",
      "test-writer",
    ]);
  });
});
