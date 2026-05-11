import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { SkillInfo } from "../../api/skills";
import { SkillsView } from "../SkillsView";

type MockProject = {
  id: string;
  name: string;
  workingDirs: string[];
};

let mockProjects: MockProject[] = [
  {
    id: "project-alpha",
    name: "alpha",
    workingDirs: ["/tmp/alpha"],
  },
];

const mockSkills: SkillInfo[] = [
  {
    id: "global:/path/layout-polish",
    name: "layout",
    description: "Improves layout, spacing, and visual hierarchy",
    instructions: "Refine spacing and visual rhythm...",
    path: "/path/layout/SKILL.md",
    fileLocation: "/path/layout/SKILL.md",
    sourceKind: "global" as const,
    sourceLabel: "Personal",
    projectLinks: [],
  },
  {
    id: "global:/path/code-review",
    name: "code-review",
    description: "Reviews code",
    instructions: "Review the code...",
    path: "/path/code-review",
    fileLocation: "/path/code-review/SKILL.md",
    sourceKind: "global" as const,
    sourceLabel: "Personal",
    projectLinks: [],
  },
  {
    id: "project:/tmp/alpha/.goose/skills/test-writer",
    name: "test-writer",
    description: "Writes tests",
    instructions: "Write tests...",
    path: "/tmp/alpha/.goose/skills/test-writer",
    fileLocation: "/tmp/alpha/.goose/skills/test-writer/SKILL.md",
    sourceKind: "project" as const,
    sourceLabel: "alpha",
    projectLinks: [
      {
        id: "/tmp/alpha",
        name: "alpha",
        workingDir: "/tmp/alpha",
      },
    ],
  },
];

const builtinSkill: SkillInfo = {
  id: "builtin:goose-doc-guide",
  name: "goose-doc-guide",
  description: "Reference Goose documentation",
  instructions: "Fetch Goose docs before answering.",
  path: "builtin://skills/goose-doc-guide",
  fileLocation: "builtin://skills/goose-doc-guide",
  sourceKind: "builtin" as const,
  sourceLabel: "Built in",
  projectLinks: [],
};

vi.mock("../../api/skills", () => ({
  listSkills: vi.fn().mockResolvedValue([]),
  createSkill: vi.fn().mockResolvedValue(undefined),
  updateSkill: vi.fn().mockResolvedValue({
    id: "global:/path/renamed-review",
    name: "renamed-review",
    description: "Reviews code",
    instructions: "Review the code...",
    path: "/path/renamed-review",
    fileLocation: "/path/renamed-review/SKILL.md",
    sourceKind: "global",
    sourceLabel: "Personal",
    projectLinks: [],
  }),
  deleteSkill: vi.fn().mockResolvedValue(undefined),
  exportSkill: vi
    .fn()
    .mockResolvedValue({ json: "{}", filename: "test.skill.json" }),
  importSkills: vi.fn().mockResolvedValue([]),
}));

vi.mock("@/features/projects/stores/projectStore", () => ({
  useProjectStore: (
    selector: (state: { projects: MockProject[] }) => unknown,
  ) => selector({ projects: mockProjects }),
}));

const { listSkills, deleteSkill, updateSkill, exportSkill } = (await import(
  "../../api/skills"
)) as unknown as {
  listSkills: ReturnType<typeof vi.fn>;
  deleteSkill: ReturnType<typeof vi.fn>;
  updateSkill: ReturnType<typeof vi.fn>;
  exportSkill: ReturnType<typeof vi.fn>;
};

beforeEach(() => {
  vi.clearAllMocks();
  mockProjects = [
    {
      id: "project-alpha",
      name: "alpha",
      workingDirs: ["/tmp/alpha"],
    },
  ];
  listSkills.mockResolvedValue([]);
});

function createDeferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error?: unknown) => void;
  const promise = new Promise<T>((promiseResolve, promiseReject) => {
    resolve = promiseResolve;
    reject = promiseReject;
  });
  return { promise, resolve, reject };
}

describe("SkillsView", () => {
  it("shows the redesigned heading and description", () => {
    render(<SkillsView />);
    expect(screen.getByText("Skills")).toBeInTheDocument();
    expect(
      screen.getByText(/Use skills to add specific instructions/),
    ).toBeInTheDocument();
  });

  it("shows the empty state when no skills are available", async () => {
    render(<SkillsView />);
    await waitFor(() => {
      expect(listSkills).toHaveBeenCalledWith(["/tmp/alpha"]);
    });
    await waitFor(() => {
      expect(screen.getByText("No skills yet")).toBeInTheDocument();
    });
    expect(
      screen.getByText("Create a skill or import one to get started."),
    ).toBeInTheDocument();
  });

  it("ignores stale skill loads after projects change", async () => {
    const firstLoad = createDeferred<typeof mockSkills>();
    const secondLoad = createDeferred<typeof mockSkills>();
    listSkills
      .mockReturnValueOnce(firstLoad.promise)
      .mockReturnValueOnce(secondLoad.promise);
    const { rerender } = render(<SkillsView />);

    await waitFor(() => {
      expect(listSkills).toHaveBeenCalledTimes(1);
    });

    mockProjects = [
      {
        id: "project-beta",
        name: "beta",
        workingDirs: ["/tmp/beta"],
      },
    ];
    rerender(<SkillsView />);

    await waitFor(() => {
      expect(listSkills).toHaveBeenCalledTimes(2);
    });

    secondLoad.resolve([
      {
        ...mockSkills[2],
        id: "project:/tmp/beta/.goose/skills/beta-skill",
        name: "beta-skill",
        path: "/tmp/beta/.goose/skills/beta-skill",
        fileLocation: "/tmp/beta/.goose/skills/beta-skill/SKILL.md",
        sourceLabel: "beta",
        projectLinks: [
          {
            id: "/tmp/beta",
            name: "beta",
            workingDir: "/tmp/beta",
          },
        ],
      },
    ]);
    await screen.findByText("beta-skill");

    firstLoad.resolve([mockSkills[2]]);
    await waitFor(() => {
      expect(screen.getByText("beta-skill")).toBeInTheDocument();
      expect(screen.queryByText("test-writer")).not.toBeInTheDocument();
    });
  });

  it("matches saved project working directories with trailing separators", async () => {
    mockProjects = [
      {
        id: "project-goose",
        name: "Goose",
        workingDirs: ["/tmp/goose/"],
      },
    ];
    listSkills.mockResolvedValue([
      {
        ...mockSkills[2],
        id: "project:/tmp/goose/.agents/skills/test-writer",
        name: "test-writer",
        path: "/tmp/goose/.agents/skills/test-writer",
        fileLocation: "/tmp/goose/.agents/skills/test-writer/SKILL.md",
        sourceLabel: "goose",
        projectLinks: [
          {
            id: "/tmp/goose",
            name: "goose",
            workingDir: "/tmp/goose",
          },
        ],
      },
    ]);
    const user = userEvent.setup();

    render(<SkillsView />);
    await screen.findByText("test-writer");

    await user.click(screen.getByRole("button", { name: "Goose" }));

    expect(screen.getByText("test-writer")).toBeInTheDocument();
  });

  it("renders skills and opens the detail subpage", async () => {
    listSkills.mockResolvedValue(mockSkills);
    const user = userEvent.setup();

    render(<SkillsView />);
    await screen.findByText("code-review");

    await user.click(
      screen.getByRole("button", { name: "Open test-writer details" }),
    );

    expect(
      screen.getByRole("button", { name: "Back to skills" }),
    ).toBeInTheDocument();
    expect(screen.getAllByText("alpha").length).toBeGreaterThan(0);
    expect(screen.getByText("Write tests...")).toBeInTheDocument();
    expect(
      screen.getByText("/tmp/alpha/.goose/skills/test-writer/SKILL.md"),
    ).toBeInTheDocument();
  });

  it("starts a chat with the selected skill from the list", async () => {
    listSkills.mockResolvedValue(mockSkills);
    const onStartChatWithSkill = vi.fn();
    const user = userEvent.setup();

    render(<SkillsView onStartChatWithSkill={onStartChatWithSkill} />);
    await screen.findByText("test-writer");

    await user.click(
      screen.getByRole("button", { name: "Start chat with test-writer" }),
    );

    expect(onStartChatWithSkill).toHaveBeenCalledWith(
      expect.objectContaining({ name: "test-writer" }),
      "project-alpha",
    );
  });

  it("starts a chat with the selected skill from the detail page", async () => {
    listSkills.mockResolvedValue(mockSkills);
    const onStartChatWithSkill = vi.fn();
    const user = userEvent.setup();

    render(<SkillsView onStartChatWithSkill={onStartChatWithSkill} />);
    await screen.findByText("code-review");

    await user.click(
      screen.getByRole("button", { name: "Open code-review details" }),
    );
    await user.click(screen.getByRole("button", { name: "Start chat" }));

    expect(onStartChatWithSkill).toHaveBeenCalledWith(
      expect.objectContaining({ name: "code-review" }),
      null,
    );
  });

  it("returns to the list without losing filters", async () => {
    listSkills.mockResolvedValue(mockSkills);
    const user = userEvent.setup();

    render(<SkillsView />);
    await screen.findByText("code-review");

    await user.click(screen.getByRole("button", { name: "alpha" }));
    await user.click(
      screen.getByRole("button", { name: "Open test-writer details" }),
    );
    await user.click(screen.getByRole("button", { name: "Back to skills" }));

    expect(screen.getByText("test-writer")).toBeInTheDocument();
    expect(screen.queryByText("code-review")).not.toBeInTheDocument();
  });

  it("stays on the detail page after renaming a skill", async () => {
    const renamedSkill: SkillInfo = {
      ...mockSkills[1],
      id: "global:/path/renamed-review",
      name: "renamed-review",
      path: "/path/renamed-review",
      fileLocation: "/path/renamed-review/SKILL.md",
    };
    listSkills
      .mockResolvedValueOnce(mockSkills)
      .mockResolvedValueOnce([mockSkills[0], renamedSkill, mockSkills[2]]);
    updateSkill.mockResolvedValueOnce(renamedSkill);
    const user = userEvent.setup();

    render(<SkillsView />);
    await screen.findByText("code-review");

    await user.click(
      screen.getByRole("button", { name: "Open code-review details" }),
    );
    await user.click(screen.getByRole("button", { name: "Edit" }));

    const nameInput = screen.getByPlaceholderText("my-skill-name");
    await user.clear(nameInput);
    await user.type(nameInput, "renamed-review");
    await user.click(screen.getByRole("button", { name: /save changes/i }));

    await waitFor(() => {
      expect(updateSkill).toHaveBeenCalledWith(
        "/path/code-review",
        "renamed-review",
        "Reviews code",
        "Review the code...",
      );
    });
    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: "Back to skills" }),
      ).toBeInTheDocument();
      expect(
        screen.getByRole("heading", { name: "renamed-review" }),
      ).toBeInTheDocument();
    });
    expect(
      screen.queryByPlaceholderText("my-skill-name"),
    ).not.toBeInTheDocument();
  });

  it("filters skills by search text", async () => {
    listSkills.mockResolvedValue(mockSkills);
    const user = userEvent.setup();

    render(<SkillsView />);
    await screen.findByText("code-review");

    await user.type(
      screen.getByPlaceholderText("Search skills"),
      "writes tests",
    );

    expect(screen.queryByText("code-review")).not.toBeInTheDocument();
    expect(screen.getByText("test-writer")).toBeInTheDocument();
  });

  it("filters skills by project from the main filter row", async () => {
    listSkills.mockResolvedValue(mockSkills);
    const user = userEvent.setup();

    render(<SkillsView />);
    await screen.findByText("code-review");

    await user.click(screen.getByRole("button", { name: "alpha" }));

    expect(screen.queryByText("code-review")).not.toBeInTheDocument();
    expect(screen.getByText("test-writer")).toBeInTheDocument();
  });

  it("groups and filters built-in skills separately", async () => {
    listSkills.mockResolvedValue([...mockSkills, builtinSkill]);
    const user = userEvent.setup();

    render(<SkillsView />);
    await screen.findByText("goose-doc-guide");

    expect(
      screen.getByRole("button", { name: "Built in" }),
    ).toBeInTheDocument();
    const sectionButtons = screen
      .getAllByRole("button")
      .filter((button) => /\d skill/.test(button.textContent ?? ""));
    expect(sectionButtons.map((button) => button.textContent)).toEqual([
      expect.stringContaining("Personal"),
      expect.stringContaining("Built in"),
      expect.stringContaining("alpha"),
    ]);

    await user.click(screen.getByRole("button", { name: "Built in" }));

    expect(screen.getByText("goose-doc-guide")).toBeInTheDocument();
    expect(screen.queryByText("code-review")).not.toBeInTheDocument();
    expect(screen.queryByText("test-writer")).not.toBeInTheDocument();
  });
  it("shows a delete confirmation from the detail panel", async () => {
    listSkills.mockResolvedValue(mockSkills);
    const user = userEvent.setup();

    render(<SkillsView />);
    await screen.findByText("code-review");

    await user.click(
      screen.getByRole("button", { name: "Open code-review details" }),
    );
    screen.getByRole("button", { name: "More" }).focus();
    await user.keyboard("{Enter}");
    await user.click(screen.getByRole("menuitem", { name: "Delete" }));

    expect(
      screen.getByText('Delete "code-review" permanently?'),
    ).toBeInTheDocument();

    const deleteButtons = screen.getAllByRole("button", { name: "Delete" });
    await user.click(deleteButtons[deleteButtons.length - 1]);

    await waitFor(() => {
      expect(deleteSkill).toHaveBeenCalledWith("/path/code-review");
    });
  });

  it("shows built-in details without filesystem actions and still starts chat", async () => {
    listSkills.mockResolvedValue([...mockSkills, builtinSkill]);
    const onStartChatWithSkill = vi.fn();
    const user = userEvent.setup();

    render(<SkillsView onStartChatWithSkill={onStartChatWithSkill} />);
    await screen.findByText("goose-doc-guide");

    await user.click(
      screen.getByRole("button", { name: "Open goose-doc-guide details" }),
    );

    expect(
      screen.getByText("Fetch Goose docs before answering."),
    ).toBeInTheDocument();
    expect(screen.queryByText("Location")).not.toBeInTheDocument();
    expect(
      screen.queryByText("builtin://skills/goose-doc-guide"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Edit" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Show in folder" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "More" }),
    ).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Start chat" }));

    expect(onStartChatWithSkill).toHaveBeenCalledWith(
      expect.objectContaining({ name: "goose-doc-guide" }),
      null,
    );
    expect(updateSkill).not.toHaveBeenCalled();
    expect(deleteSkill).not.toHaveBeenCalled();
    expect(exportSkill).not.toHaveBeenCalled();
  });

  it("passes saved project working directories into listSkills", async () => {
    mockProjects = [
      {
        id: "project-goose",
        name: "Goose",
        workingDirs: ["/tmp/goose", "/tmp/goose-worktree"],
      },
    ];

    render(<SkillsView />);

    await waitFor(() => {
      expect(listSkills).toHaveBeenCalledWith([
        "/tmp/goose",
        "/tmp/goose-worktree",
      ]);
    });
  });

  it("groups multiple working directories under the saved project filter", async () => {
    mockProjects = [
      {
        id: "project-goose",
        name: "Goose",
        workingDirs: ["/tmp/goose", "/tmp/goose-worktree"],
      },
    ];
    listSkills.mockResolvedValue([
      {
        ...mockSkills[2],
        id: "project:/tmp/goose/.agents/skills/test-writer",
        name: "test-writer",
        path: "/tmp/goose/.agents/skills/test-writer",
        fileLocation: "/tmp/goose/.agents/skills/test-writer/SKILL.md",
        sourceLabel: "goose",
        projectLinks: [
          {
            id: "/tmp/goose",
            name: "goose",
            workingDir: "/tmp/goose",
          },
        ],
      },
      {
        ...mockSkills[2],
        id: "project:/tmp/goose-worktree/.agents/skills/audit",
        name: "audit",
        path: "/tmp/goose-worktree/.agents/skills/audit",
        fileLocation: "/tmp/goose-worktree/.agents/skills/audit/SKILL.md",
        sourceLabel: "goose-worktree",
        projectLinks: [
          {
            id: "/tmp/goose-worktree",
            name: "goose-worktree",
            workingDir: "/tmp/goose-worktree",
          },
        ],
      },
    ]);
    const user = userEvent.setup();

    render(<SkillsView />);
    await screen.findByText("test-writer");

    await user.click(screen.getByRole("button", { name: "Goose" }));

    expect(screen.getByText("test-writer")).toBeInTheDocument();
    expect(screen.getByText("audit")).toBeInTheDocument();
  });
});
