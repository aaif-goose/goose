import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SkillsView } from "../SkillsView";

const mockSkills = [
  {
    id: "global:/path/code-review",
    name: "code-review",
    description: "Reviews code",
    instructions: "Review the code...",
    path: "/path/code-review",
    fileLocation: "/path/code-review/SKILL.md",
    directoryPath: "/path/code-review",
    sourceKind: "global" as const,
    sourceLabel: "Personal",
    projectLinks: [],
    editable: true,
  },
  {
    id: "project:/tmp/alpha/.goose/skills/test-writer",
    name: "test-writer",
    description: "Writes tests",
    instructions: "Write tests...",
    path: "/tmp/alpha/.goose/skills/test-writer",
    fileLocation: "/tmp/alpha/.goose/skills/test-writer/SKILL.md",
    directoryPath: "/tmp/alpha/.goose/skills/test-writer",
    sourceKind: "project" as const,
    sourceLabel: "alpha",
    projectLinks: [
      {
        id: "/tmp/alpha",
        name: "alpha",
        workingDir: "/tmp/alpha",
      },
    ],
    editable: true,
  },
];

vi.mock("../../api/skills", () => ({
  listSkills: vi.fn().mockResolvedValue([]),
  deleteSkill: vi.fn().mockResolvedValue(undefined),
  exportSkill: vi
    .fn()
    .mockResolvedValue({ json: "{}", filename: "test.skill.json" }),
  importSkills: vi.fn().mockResolvedValue([]),
}));

const { listSkills, deleteSkill } = (await import(
  "../../api/skills"
)) as unknown as {
  listSkills: ReturnType<typeof vi.fn>;
  deleteSkill: ReturnType<typeof vi.fn>;
};

beforeEach(() => {
  vi.clearAllMocks();
  listSkills.mockResolvedValue([]);
});

describe("SkillsView", () => {
  it("shows the redesigned heading and description", () => {
    render(<SkillsView />);
    expect(screen.getByText("Skills")).toBeInTheDocument();
    expect(
      screen.getByText(/Skills are reusable instructions/),
    ).toBeInTheDocument();
  });

  it("shows the empty state when no skills are available", async () => {
    render(<SkillsView />);
    await waitFor(() => {
      expect(screen.getByText("No skills yet")).toBeInTheDocument();
    });
    expect(
      screen.getByText("Create a skill or import one to get started."),
    ).toBeInTheDocument();
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
      screen.getByText("/tmp/alpha/.goose/skills/test-writer"),
    ).toBeInTheDocument();
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

  it("shows a delete confirmation from the detail panel", async () => {
    listSkills.mockResolvedValue(mockSkills);
    const user = userEvent.setup();

    render(<SkillsView />);
    await screen.findByText("code-review");

    await user.click(
      screen.getByRole("button", { name: "Open code-review details" }),
    );
    await user.click(screen.getByRole("button", { name: "Delete" }));

    expect(screen.getByText("Delete skill?")).toBeInTheDocument();

    const deleteButtons = screen.getAllByRole("button", { name: "Delete" });
    await user.click(deleteButtons[deleteButtons.length - 1]);

    await waitFor(() => {
      expect(deleteSkill).toHaveBeenCalledWith("/path/code-review");
    });
  });
});
