import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SkillsView } from "../SkillsView";

const mockSkills = [
  {
    name: "code-review",
    description: "Reviews code",
    instructions: "Review the code...",
    path: "/path",
  },
  {
    name: "test-writer",
    description: "Writes tests",
    instructions: "Write tests...",
    path: "/path",
  },
];

vi.mock("../../api/skills", () => ({
  getCachedSkills: vi.fn(() => []),
  primeSkillsCache: vi.fn(),
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
    it("shows the new skill tile", async () => {
      const { container } = render(<SkillsView />);

      await waitFor(() => {
        expect(container.querySelector(".animate-pulse")).toBeNull();
      });

      expect(
        screen.getByRole("button", { name: "New Skill" }),
      ).toBeInTheDocument();
    });

    it("opens the create dialog from the new skill tile", async () => {
      const user = userEvent.setup();
      render(<SkillsView />);

      await user.click(screen.getByRole("button", { name: "New Skill" }));

      expect(screen.getByRole("dialog")).toBeInTheDocument();
      expect(
        screen.getByRole("heading", { name: "New Skill" }),
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

  describe("Skill grid", () => {
    it("renders every returned skill", async () => {
      listSkills.mockResolvedValue(mockSkills);
      render(<SkillsView />);

      expect(await screen.findByText("code-review")).toBeInTheDocument();
      expect(screen.getByText("test-writer")).toBeInTheDocument();
    });

    it("shows install placeholders for returned skills", async () => {
      listSkills.mockResolvedValue(mockSkills);
      render(<SkillsView />);

      expect(
        await screen.findByLabelText("Install code-review (placeholder)"),
      ).toBeInTheDocument();
      expect(
        screen.getByLabelText("Install test-writer (placeholder)"),
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
        expect(deleteSkill).toHaveBeenCalledWith("code-review");
      });
    });
  });
});
