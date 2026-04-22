import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SkillsView } from "../SkillsView";

const mockSkills = [
  {
    name: "code-review",
    description: "Reviews code",
    instructions: "Review the code...",
    path: "/path/code-review",
    editable: true,
    fileLocation: "/path/code-review/SKILL.md",
  },
  {
    name: "test-writer",
    description: "Writes tests",
    instructions: "Write tests...",
    path: "/path/test-writer",
    editable: true,
    fileLocation: "/path/test-writer/SKILL.md",
  },
];

vi.mock("../../api/skills", () => ({
  listSkills: vi.fn().mockResolvedValue([]),
  createSkill: vi.fn().mockResolvedValue(undefined),
  deleteSkill: vi.fn().mockResolvedValue(undefined),
  updateSkill: vi.fn().mockResolvedValue(undefined),
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
  describe("Rendering", () => {
    it("shows Skills heading and subtitle", async () => {
      render(<SkillsView />);
      expect(screen.getByText("Skills")).toBeInTheDocument();
      expect(
        screen.getByText("Reusable instructions for your AI agents"),
      ).toBeInTheDocument();
    });

    it("shows New Skill and Import buttons", async () => {
      render(<SkillsView />);
      expect(screen.getByText("New Skill")).toBeInTheDocument();
      expect(screen.getByText("Import")).toBeInTheDocument();
    });

    it("shows empty state when no skills exist", async () => {
      render(<SkillsView />);
      await waitFor(() => {
        expect(screen.getByText("No skills yet")).toBeInTheDocument();
      });
      expect(
        screen.getByText("Create a skill or drop a .skill.json file here."),
      ).toBeInTheDocument();
    });

    it("renders skill cards when skills are loaded", async () => {
      listSkills.mockResolvedValue(mockSkills);
      render(<SkillsView />);
      expect(await screen.findByText("code-review")).toBeInTheDocument();
      expect(screen.getByText("test-writer")).toBeInTheDocument();
      expect(screen.getByText("Reviews code")).toBeInTheDocument();
      expect(screen.getByText("Writes tests")).toBeInTheDocument();
    });
  });

  describe("Search", () => {
    it("filters skills by name when searching", async () => {
      listSkills.mockResolvedValue(mockSkills);
      const user = userEvent.setup();
      render(<SkillsView />);
      await screen.findByText("code-review");

      await user.type(
        screen.getByPlaceholderText("Search skills by name or description..."),
        "code",
      );

      expect(screen.getByText("code-review")).toBeInTheDocument();
      expect(screen.queryByText("test-writer")).not.toBeInTheDocument();
    });

    it("filters skills by description when searching", async () => {
      listSkills.mockResolvedValue(mockSkills);
      const user = userEvent.setup();
      render(<SkillsView />);
      await screen.findByText("code-review");

      await user.type(
        screen.getByPlaceholderText("Search skills by name or description..."),
        "Writes tests",
      );

      expect(screen.queryByText("code-review")).not.toBeInTheDocument();
      expect(screen.getByText("test-writer")).toBeInTheDocument();
    });

    it("shows empty state when search has no results", async () => {
      listSkills.mockResolvedValue(mockSkills);
      const user = userEvent.setup();
      render(<SkillsView />);
      await screen.findByText("code-review");

      await user.type(
        screen.getByPlaceholderText("Search skills by name or description..."),
        "nonexistent",
      );

      expect(screen.getByText("No matching skills")).toBeInTheDocument();
      expect(
        screen.getByText("Try a different search term."),
      ).toBeInTheDocument();
    });
  });

  describe("Skill card menu", () => {
    it("shows dropdown menu with Edit, Duplicate, Export, Delete options", async () => {
      listSkills.mockResolvedValue(mockSkills);
      const user = userEvent.setup();
      render(<SkillsView />);
      await screen.findByText("code-review");

      await user.click(screen.getByLabelText("Options for code-review"));

      expect(screen.getByRole("menu")).toBeInTheDocument();
      expect(
        screen.getByRole("menuitem", { name: /edit/i }),
      ).toBeInTheDocument();
      expect(
        screen.getByRole("menuitem", { name: /duplicate/i }),
      ).toBeInTheDocument();
      expect(
        screen.getByRole("menuitem", { name: /export/i }),
      ).toBeInTheDocument();
      expect(
        screen.getByRole("menuitem", { name: /delete/i }),
      ).toBeInTheDocument();
    });
  });

  describe("Delete confirmation", () => {
    it("shows confirmation dialog when delete is clicked", async () => {
      listSkills.mockResolvedValue(mockSkills);
      const user = userEvent.setup();
      render(<SkillsView />);
      await screen.findByText("code-review");

      await user.click(screen.getByLabelText("Options for code-review"));
      await user.click(screen.getByRole("menuitem", { name: /delete/i }));

      expect(screen.getByText("Delete skill?")).toBeInTheDocument();
      expect(
        screen.getByText(/Are you sure you want to delete "code-review"\?/),
      ).toBeInTheDocument();
    });

    it("cancels deletion when Cancel is clicked", async () => {
      listSkills.mockResolvedValue(mockSkills);
      const user = userEvent.setup();
      render(<SkillsView />);
      await screen.findByText("code-review");

      await user.click(screen.getByLabelText("Options for code-review"));
      await user.click(screen.getByRole("menuitem", { name: /delete/i }));
      expect(screen.getByText("Delete skill?")).toBeInTheDocument();

      await user.click(screen.getByText("Cancel"));
      expect(screen.queryByText("Delete skill?")).not.toBeInTheDocument();
    });

    it("calls deleteSkill API when confirmed", async () => {
      listSkills.mockResolvedValue(mockSkills);
      const user = userEvent.setup();
      render(<SkillsView />);
      await screen.findByText("code-review");

      await user.click(screen.getByLabelText("Options for code-review"));
      await user.click(screen.getByRole("menuitem", { name: /delete/i }));
      await user.click(screen.getByRole("button", { name: "Delete" }));

      await waitFor(() => {
        expect(deleteSkill).toHaveBeenCalledWith("/path/code-review");
      });
    });

    it("shows read-only state and file location for non-editable skills", async () => {
      listSkills.mockResolvedValue([
        {
          name: "claude-skill",
          description: "Imported from Claude",
          instructions: "Use this skill...",
          path: "/Users/test/.claude/skills/claude-skill",
          editable: false,
          fileLocation: "/Users/test/.claude/skills/claude-skill/SKILL.md",
        },
      ]);

      render(<SkillsView />);

      expect(await screen.findByText("claude-skill")).toBeInTheDocument();
      expect(screen.getByText("Read-only")).toBeInTheDocument();
      expect(
        screen.getByText(
          "This skill was discovered outside Goose’s managed skills folders. Edit the file directly at:",
        ),
      ).toBeInTheDocument();
      expect(
        screen.getByText("/Users/test/.claude/skills/claude-skill/SKILL.md"),
      ).toBeInTheDocument();
      expect(
        screen.queryByLabelText("Options for claude-skill"),
      ).not.toBeInTheDocument();
    });

    it("shows ~/.agents/skills entries as editable when returned editable by the API", async () => {
      listSkills.mockResolvedValue([
        {
          name: "managed-skill",
          description: "Managed by Goose",
          instructions: "Use this skill...",
          path: "/Users/test/.agents/skills/managed-skill",
          editable: true,
          fileLocation: "/Users/test/.agents/skills/managed-skill/SKILL.md",
        },
      ]);
      const user = userEvent.setup();

      render(<SkillsView />);

      expect(await screen.findByText("managed-skill")).toBeInTheDocument();
      expect(screen.queryByText("Read-only")).not.toBeInTheDocument();

      await user.click(screen.getByLabelText("Options for managed-skill"));
      expect(
        screen.getByRole("menuitem", { name: /edit/i }),
      ).toBeInTheDocument();
    });
  });
});
