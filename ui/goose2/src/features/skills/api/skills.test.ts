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
            path: "/Users/test/.agents/skills/code-review",
            global: true,
          },
        ],
      })
      .mockResolvedValueOnce({ sources: [] })
      .mockResolvedValueOnce({
        sources: [
          {
            type: "skill",
            name: "code-review",
            description: "Reviews code",
            content: "Review carefully",
            path: "/Users/test/.agents/skills/code-review",
            global: true,
          },
          {
            type: "skill",
            name: "test-writer",
            description: "Writes tests",
            content: "Write tests",
            path: "/tmp/alpha/.agents/skills/test-writer",
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
      type: "builtinSkill",
    });
    expect(mockGooseSourcesList).toHaveBeenNthCalledWith(3, {
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
      .mockResolvedValueOnce({ sources: [] })
      .mockResolvedValueOnce({
        sources: [
          {
            type: "skill",
            name: "legacy-writer",
            description: "Legacy project skill",
            content: "Legacy instructions",
            path: "/tmp/beta/.goose/skills/legacy-writer",
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
            path: "/Users/test/.agents/skills/code-review",
            global: true,
          },
        ],
      })
      .mockResolvedValueOnce({ sources: [] })
      .mockRejectedValueOnce(new Error("permission denied"))
      .mockResolvedValueOnce({
        sources: [
          {
            type: "skill",
            name: "test-writer",
            description: "Writes tests",
            content: "Write tests",
            path: "/tmp/beta/.agents/skills/test-writer",
            global: false,
          },
        ],
      });

    const { listSkills } = await import("./skills");
    const skills = await listSkills(["/tmp/alpha", "/tmp/beta"]);

    expect(mockGooseSourcesList).toHaveBeenCalledTimes(4);
    expect(skills.map((skill) => skill.name)).toEqual([
      "code-review",
      "test-writer",
    ]);
  });

  it("keeps filesystem skills when built-in skill listing fails", async () => {
    mockGooseSourcesList
      .mockResolvedValueOnce({
        sources: [
          {
            type: "skill",
            name: "code-review",
            description: "Reviews code",
            content: "Review carefully",
            path: "/Users/test/.agents/skills/code-review",
            global: true,
          },
        ],
      })
      .mockRejectedValueOnce(new Error("unknown source type"))
      .mockResolvedValueOnce({
        sources: [
          {
            type: "skill",
            name: "test-writer",
            description: "Writes tests",
            content: "Write tests",
            path: "/tmp/alpha/.agents/skills/test-writer",
            global: false,
          },
        ],
      });

    const { listSkills } = await import("./skills");
    const skills = await listSkills(["/tmp/alpha"]);

    expect(mockGooseSourcesList).toHaveBeenNthCalledWith(1, {
      type: "skill",
    });
    expect(mockGooseSourcesList).toHaveBeenNthCalledWith(2, {
      type: "builtinSkill",
    });
    expect(mockGooseSourcesList).toHaveBeenNthCalledWith(3, {
      type: "skill",
      projectDir: "/tmp/alpha",
    });
    expect(skills.map((skill) => skill.name)).toEqual([
      "code-review",
      "test-writer",
    ]);
  });

  it("fetches and maps built-in skills without filesystem project/global metadata", async () => {
    mockGooseSourcesList
      .mockResolvedValueOnce({
        sources: [
          {
            type: "skill",
            name: "personal-review",
            description: "Reviews personal code",
            content: "Review local changes",
            path: "/Users/test/.agents/skills/personal-review",
            global: true,
          },
        ],
      })
      .mockResolvedValueOnce({
        sources: [
          {
            type: "builtinSkill",
            name: "goose-doc-guide",
            description: "Goose documentation guide",
            content: "Use Goose docs",
            path: "builtin://skills/goose-doc-guide",
            global: true,
          },
        ],
      })
      .mockResolvedValueOnce({
        sources: [
          {
            type: "skill",
            name: "project-helper",
            description: "Helps project work",
            content: "Use project context",
            path: "/tmp/alpha/.agents/skills/project-helper",
            global: false,
          },
        ],
      });

    const { listSkills } = await import("./skills");
    const skills = await listSkills(["/tmp/alpha"]);

    expect(mockGooseSourcesList).toHaveBeenNthCalledWith(1, {
      type: "skill",
    });
    expect(mockGooseSourcesList).toHaveBeenNthCalledWith(2, {
      type: "builtinSkill",
    });
    expect(mockGooseSourcesList).toHaveBeenNthCalledWith(3, {
      type: "skill",
      projectDir: "/tmp/alpha",
    });
    expect(skills.map((skill) => skill.name)).toEqual([
      "personal-review",
      "goose-doc-guide",
      "project-helper",
    ]);
    expect(skills).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: "builtin:goose-doc-guide",
          name: "goose-doc-guide",
          path: "builtin://skills/goose-doc-guide",
          fileLocation: "builtin://skills/goose-doc-guide",
          sourceKind: "builtin",
          sourceLabel: "Built in",
          projectLinks: [],
        }),
      ]),
    );
  });
});
