import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi, beforeEach } from "vitest";
import { CreateSkillDialog } from "../CreateSkillDialog";

vi.mock("../../api/skills", () => ({
  createSkill: vi.fn().mockResolvedValue(undefined),
  updateSkill: vi.fn().mockResolvedValue({
    name: "test",
    description: "test",
    instructions: "",
    path: "",
  }),
}));

// eslint-disable-next-line @typescript-eslint/no-require-imports
const { createSkill, updateSkill } = await import("../../api/skills");

const defaultProps = {
  isOpen: true,
  onClose: vi.fn(),
  onCreated: vi.fn(),
};

describe("CreateSkillDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  // ── Rendering ──────────────────────────────────────────────────────

  describe("rendering", () => {
    it("does not render when isOpen is false", () => {
      render(<CreateSkillDialog {...defaultProps} isOpen={false} />);
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });

    it("renders dialog when isOpen is true", () => {
      render(<CreateSkillDialog {...defaultProps} />);
      expect(screen.getByRole("dialog")).toBeInTheDocument();
    });

    it('shows "New Skill" title in create mode', () => {
      render(<CreateSkillDialog {...defaultProps} />);
      expect(screen.getByText("New Skill")).toBeInTheDocument();
    });

    it('shows "Edit Skill" title when editingSkill is provided', () => {
      render(
        <CreateSkillDialog
          {...defaultProps}
          editingSkill={{
            name: "my-skill",
            description: "desc",
            instructions: "instr",
          }}
        />,
      );
      expect(screen.getByText("Edit Skill")).toBeInTheDocument();
    });
  });

  // ── Name validation ────────────────────────────────────────────────

  describe("name validation", () => {
    it("allows valid kebab-case names", async () => {
      const user = userEvent.setup();
      render(<CreateSkillDialog {...defaultProps} />);
      const nameInput = screen.getByPlaceholderText("my-skill-name");

      await user.type(nameInput, "my-skill");
      expect(nameInput).toHaveValue("my-skill");
      expect(screen.queryByText(/must be kebab-case/i)).not.toBeInTheDocument();
    });

    it("auto-formats input (uppercase to lowercase, spaces to hyphens)", async () => {
      const user = userEvent.setup();
      render(<CreateSkillDialog {...defaultProps} />);
      const nameInput = screen.getByPlaceholderText("my-skill-name");

      await user.type(nameInput, "My Skill");
      expect(nameInput).toHaveValue("my-skill");
    });

    it("allows typing hyphens", async () => {
      const user = userEvent.setup();
      render(<CreateSkillDialog {...defaultProps} />);
      const nameInput = screen.getByPlaceholderText("my-skill-name");

      await user.type(nameInput, "code-review");
      expect(nameInput).toHaveValue("code-review");
    });

    it("shows validation error for invalid name with trailing hyphen", async () => {
      const user = userEvent.setup();
      render(<CreateSkillDialog {...defaultProps} />);
      const nameInput = screen.getByPlaceholderText("my-skill-name");

      // Type a single hyphen — the formatter strips leading hyphens,
      // but we can produce an invalid state by clearing and typing a
      // non-kebab string. Actually the formatter is aggressive, so let's
      // just check that when name is non-empty but invalid, the error shows.
      // We type "a-" which gives "a-" — valid prefix but trailing hyphen fails regex.
      await user.type(nameInput, "a-");
      expect(nameInput).toHaveValue("a-");
      expect(screen.getByText(/must be kebab-case/i)).toBeInTheDocument();
    });

    it("save button is disabled when name is empty", () => {
      render(<CreateSkillDialog {...defaultProps} />);
      const saveButton = screen.getByRole("button", { name: /create skill/i });
      expect(saveButton).toBeDisabled();
    });
  });

  // ── Edit mode ──────────────────────────────────────────────────────

  describe("edit mode", () => {
    const editingSkill = {
      name: "code-review",
      description: "Reviews code",
      instructions: "Review the code carefully",
    };

    it("pre-fills fields with existing skill data", () => {
      render(
        <CreateSkillDialog {...defaultProps} editingSkill={editingSkill} />,
      );
      expect(screen.getByPlaceholderText("my-skill-name")).toHaveValue(
        "code-review",
      );
      expect(
        screen.getByPlaceholderText("What it does and when to use it..."),
      ).toHaveValue("Reviews code");
      expect(
        screen.getByPlaceholderText(
          "Markdown instructions the agent will follow...",
        ),
      ).toHaveValue("Review the code carefully");
    });

    it("name field is read-only in edit mode", () => {
      render(
        <CreateSkillDialog {...defaultProps} editingSkill={editingSkill} />,
      );
      const nameInput = screen.getByPlaceholderText("my-skill-name");
      expect(nameInput).toHaveAttribute("readOnly");
    });

    it('save button text is "Save Changes" in edit mode', () => {
      render(
        <CreateSkillDialog {...defaultProps} editingSkill={editingSkill} />,
      );
      expect(
        screen.getByRole("button", { name: /save changes/i }),
      ).toBeInTheDocument();
    });
  });

  // ── Form submission ────────────────────────────────────────────────

  describe("form submission", () => {
    it("calls createSkill API on save in create mode", async () => {
      const user = userEvent.setup();
      render(<CreateSkillDialog {...defaultProps} />);

      await user.type(screen.getByPlaceholderText("my-skill-name"), "my-skill");
      await user.type(
        screen.getByPlaceholderText("What it does and when to use it..."),
        "A description",
      );
      await user.type(
        screen.getByPlaceholderText(
          "Markdown instructions the agent will follow...",
        ),
        "Some instructions",
      );
      await user.click(screen.getByRole("button", { name: /create skill/i }));

      expect(createSkill).toHaveBeenCalledWith(
        "my-skill",
        "A description",
        "Some instructions",
      );
    });

    it("calls updateSkill API on save in edit mode", async () => {
      const user = userEvent.setup();
      render(
        <CreateSkillDialog
          {...defaultProps}
          editingSkill={{
            name: "code-review",
            description: "Reviews code",
            instructions: "Review carefully",
          }}
        />,
      );

      // Change description
      const descInput = screen.getByPlaceholderText(
        "What it does and when to use it...",
      );
      await user.clear(descInput);
      await user.type(descInput, "Updated description");

      await user.click(screen.getByRole("button", { name: /save changes/i }));

      expect(updateSkill).toHaveBeenCalledWith(
        "code-review",
        "Updated description",
        "Review carefully",
      );
    });

    it("calls onCreated callback after successful save", async () => {
      const user = userEvent.setup();
      const onCreated = vi.fn();
      render(<CreateSkillDialog {...defaultProps} onCreated={onCreated} />);

      await user.type(screen.getByPlaceholderText("my-skill-name"), "my-skill");
      await user.type(
        screen.getByPlaceholderText("What it does and when to use it..."),
        "desc",
      );
      await user.click(screen.getByRole("button", { name: /create skill/i }));

      expect(onCreated).toHaveBeenCalled();
    });

    it("clears fields after save", async () => {
      const user = userEvent.setup();
      // Re-render with isOpen toggling to verify fields are cleared
      const { rerender } = render(<CreateSkillDialog {...defaultProps} />);

      await user.type(screen.getByPlaceholderText("my-skill-name"), "my-skill");
      await user.type(
        screen.getByPlaceholderText("What it does and when to use it..."),
        "desc",
      );
      await user.click(screen.getByRole("button", { name: /create skill/i }));

      // Dialog closes after save; reopen to check fields are cleared
      rerender(<CreateSkillDialog {...defaultProps} />);

      expect(screen.getByPlaceholderText("my-skill-name")).toHaveValue("");
      expect(
        screen.getByPlaceholderText("What it does and when to use it..."),
      ).toHaveValue("");
    });

    it("shows error message on save failure", async () => {
      const user = userEvent.setup();
      vi.mocked(createSkill).mockRejectedValueOnce(new Error("Network error"));

      render(<CreateSkillDialog {...defaultProps} />);

      await user.type(screen.getByPlaceholderText("my-skill-name"), "my-skill");
      await user.type(
        screen.getByPlaceholderText("What it does and when to use it..."),
        "desc",
      );
      await user.click(screen.getByRole("button", { name: /create skill/i }));

      expect(screen.getByText(/network error/i)).toBeInTheDocument();
    });
  });
});
