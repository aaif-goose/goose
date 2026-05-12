import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi, beforeEach } from "vitest";
import { open } from "@tauri-apps/plugin-dialog";
import { readProjectIcon, type ProjectInfo } from "../../api/projects";
import { CreateProjectDialog } from "../CreateProjectDialog";

// ── ResizeObserver polyfill (needed by Radix Select in jsdom) ────────

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??=
  ResizeObserverStub as unknown as typeof ResizeObserver;

// ── Mocks ────────────────────────────────────────────────────────────

vi.mock("@/shared/api/acp", () => ({
  discoverAcpProviders: vi.fn().mockResolvedValue([]),
}));

vi.mock("@/shared/api/system", () => ({
  getHomeDir: vi.fn().mockResolvedValue("/home/user"),
}));

vi.mock("../../api/projects", () => ({
  createProject: vi.fn().mockResolvedValue({
    id: "new-1",
    path: "/tmp/projects/new-1.md",
    name: "Test",
    description: "",
    prompt: "",
    icon: "tabler:folder-code",
    color: "#64748b",
    preferredProvider: null,
    preferredModel: null,
    workingDirs: [],
    useWorktrees: false,
    order: 0,
    archivedAt: null,
  }),
  updateProject: vi.fn().mockResolvedValue({
    id: "proj-1",
    path: "/tmp/projects/proj-1.md",
    name: "Updated",
    description: "",
    prompt: "",
    icon: "tabler:folder-code",
    color: "#ef4444",
    preferredProvider: null,
    preferredModel: null,
    workingDirs: [],
    useWorktrees: false,
    order: 0,
    archivedAt: null,
  }),
  scanProjectIcons: vi.fn().mockResolvedValue([]),
  readProjectIcon: vi.fn().mockResolvedValue({
    icon: "data:image/png;base64,aWNvbg==",
  }),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn().mockResolvedValue(null),
}));

// Mock PromptEditor to a simple textarea for easier testing
vi.mock("../PromptEditor", () => ({
  PromptEditor: ({
    value,
    onChange,
    placeholder,
  }: {
    value: string;
    onChange: (v: string) => void;
    placeholder?: string;
  }) => (
    <textarea
      data-testid="prompt-editor"
      value={value}
      onChange={(e) => onChange(e.target.value)}
      placeholder={placeholder}
    />
  ),
}));

// ── Helpers ───────────────────────────────────────────────────────────

function makeEditingProject(overrides: Partial<ProjectInfo> = {}): ProjectInfo {
  return {
    id: "proj-1",
    path: "/tmp/projects/proj-1.md",
    name: "My Project",
    description: "A test project",
    prompt: "Do the thing",
    icon: "tabler:folder-code",
    color: "#ef4444",
    preferredProvider: null,
    preferredModel: null,
    workingDirs: ["/home/user/code"],
    useWorktrees: false,
    order: 0,
    archivedAt: null,
    ...overrides,
  };
}

const defaultProps = {
  isOpen: true,
  onClose: vi.fn(),
  onCreated: vi.fn(),
};

// ── Tests ─────────────────────────────────────────────────────────────

describe("CreateProjectDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(open).mockResolvedValue(null);
    vi.mocked(readProjectIcon).mockResolvedValue({
      icon: "data:image/png;base64,aWNvbg==",
    });
  });

  // ── Form populates on open ──────────────────────────────────────────

  describe("form populates on open", () => {
    it("populates the name field when opening with an editingProject", () => {
      const editingProject = makeEditingProject();

      render(
        <CreateProjectDialog
          {...defaultProps}
          isOpen={true}
          editingProject={editingProject}
        />,
      );

      const nameInput = screen.getByPlaceholderText("My Project");
      expect(nameInput).toHaveValue("My Project");
    });

    it("shows Edit Project title when editingProject is provided", () => {
      render(
        <CreateProjectDialog
          {...defaultProps}
          isOpen={true}
          editingProject={makeEditingProject()}
        />,
      );

      expect(screen.getByText("Edit project")).toBeInTheDocument();
    });

    it("shows New Project title without editingProject", () => {
      render(<CreateProjectDialog {...defaultProps} isOpen={true} />);

      expect(screen.getByText("New project")).toBeInTheDocument();
    });

    it("populates the prompt editor with working dirs and prompt text", () => {
      const editingProject = makeEditingProject({
        workingDirs: ["/home/user/code"],
        prompt: "Do the thing",
      });

      render(
        <CreateProjectDialog
          {...defaultProps}
          isOpen={true}
          editingProject={editingProject}
        />,
      );

      const promptEditor = screen.getByTestId("prompt-editor");
      // buildEditorText puts prompt first, then blank line, then include lines
      expect(promptEditor).toHaveValue(
        "Do the thing\n\ninclude: /home/user/code",
      );
    });

    it("selects the correct icon from editingProject", () => {
      const editingProject = makeEditingProject({ icon: "tabler:code" });

      render(
        <CreateProjectDialog
          {...defaultProps}
          isOpen={true}
          editingProject={editingProject}
        />,
      );

      const iconButton = screen.getByRole("button", {
        name: "Icon Code",
      });
      expect(iconButton.className).toContain("border-foreground");
    });

    it("shows custom icon upload errors", async () => {
      const user = userEvent.setup();
      vi.mocked(open).mockResolvedValueOnce("/tmp/large-icon.png");
      vi.mocked(readProjectIcon).mockRejectedValueOnce(
        "Icon file is too large",
      );

      render(<CreateProjectDialog {...defaultProps} isOpen={true} />);

      await user.click(screen.getByRole("button", { name: "Custom icon" }));

      expect(await screen.findByText("Icon file is too large")).toBeVisible();
    });
  });

  // ── Form does NOT reset on re-render (the bug fix) ──────────────────

  describe("form does NOT reset on re-render while open", () => {
    it("preserves typed name when editingProject reference changes but dialog stays open", async () => {
      const user = userEvent.setup();
      const editingProject1 = makeEditingProject();

      const { rerender } = render(
        <CreateProjectDialog
          {...defaultProps}
          isOpen={true}
          editingProject={editingProject1}
        />,
      );

      // The name field should be populated
      const nameInput = screen.getByPlaceholderText("My Project");
      expect(nameInput).toHaveValue("My Project");

      // User types additional text
      await user.clear(nameInput);
      await user.type(nameInput, "Modified Name");
      expect(nameInput).toHaveValue("Modified Name");

      // Re-render with a NEW object reference but same values.
      // This simulates what happens when a parent re-renders and creates
      // a new editingProject object inline.
      const editingProject2 = makeEditingProject();
      expect(editingProject1).not.toBe(editingProject2); // different references

      rerender(
        <CreateProjectDialog
          {...defaultProps}
          isOpen={true}
          editingProject={editingProject2}
        />,
      );

      // The typed text should be preserved, NOT reset to "My Project"
      expect(nameInput).toHaveValue("Modified Name");
    });

    it("preserves edited prompt when editingProject reference changes but dialog stays open", async () => {
      const user = userEvent.setup();
      const editingProject1 = makeEditingProject();

      const { rerender } = render(
        <CreateProjectDialog
          {...defaultProps}
          isOpen={true}
          editingProject={editingProject1}
        />,
      );

      const promptEditor = screen.getByTestId("prompt-editor");
      expect(promptEditor).toHaveValue(
        "Do the thing\n\ninclude: /home/user/code",
      );

      // User modifies the prompt
      await user.clear(promptEditor);
      await user.type(promptEditor, "New instructions");
      expect(promptEditor).toHaveValue("New instructions");

      // Re-render with new reference, same values
      const editingProject2 = makeEditingProject();

      rerender(
        <CreateProjectDialog
          {...defaultProps}
          isOpen={true}
          editingProject={editingProject2}
        />,
      );

      // The edited prompt should be preserved
      expect(promptEditor).toHaveValue("New instructions");
    });

    it("preserves changed icon when editingProject reference changes but dialog stays open", async () => {
      const user = userEvent.setup();
      const editingProject1 = makeEditingProject({ icon: "tabler:code" });

      const { rerender } = render(
        <CreateProjectDialog
          {...defaultProps}
          isOpen={true}
          editingProject={editingProject1}
        />,
      );

      const codeButton = screen.getByRole("button", {
        name: "Icon Code",
      });
      expect(codeButton.className).toContain("border-foreground");

      const terminalButton = screen.getByRole("button", {
        name: "Icon Terminal",
      });
      await user.click(terminalButton);
      expect(terminalButton.className).toContain("border-foreground");
      expect(codeButton.className).not.toContain("border-foreground");

      // Re-render with new reference, same values
      const editingProject2 = makeEditingProject({ icon: "tabler:code" });

      rerender(
        <CreateProjectDialog
          {...defaultProps}
          isOpen={true}
          editingProject={editingProject2}
        />,
      );

      expect(terminalButton.className).toContain("border-foreground");
      expect(codeButton.className).not.toContain("border-foreground");
    });
  });

  // ── Form populates again on close and reopen ─────────────────────────

  describe("form populates again on close and reopen", () => {
    it("re-populates fields when dialog closes and reopens with a different project", async () => {
      const project1 = makeEditingProject({
        name: "Project Alpha",
        icon: "tabler:code",
        prompt: "Alpha instructions",
        workingDirs: ["/alpha"],
      });

      const { rerender } = render(
        <CreateProjectDialog
          {...defaultProps}
          isOpen={true}
          editingProject={project1}
        />,
      );

      // Verify initial population
      expect(screen.getByPlaceholderText("My Project")).toHaveValue(
        "Project Alpha",
      );

      // Close the dialog
      rerender(
        <CreateProjectDialog
          {...defaultProps}
          isOpen={false}
          editingProject={project1}
        />,
      );

      // Reopen with a different project
      const project2 = makeEditingProject({
        name: "Project Beta",
        icon: "tabler:database",
        prompt: "Beta instructions",
        workingDirs: ["/beta"],
      });

      rerender(
        <CreateProjectDialog
          {...defaultProps}
          isOpen={true}
          editingProject={project2}
        />,
      );

      // Fields should now have the new project's data
      const nameInput = screen.getByPlaceholderText("My Project");
      expect(nameInput).toHaveValue("Project Beta");

      const promptEditor = screen.getByTestId("prompt-editor");
      expect(promptEditor).toHaveValue("Beta instructions\n\ninclude: /beta");

      const databaseButton = screen.getByRole("button", {
        name: "Icon Database",
      });
      expect(databaseButton.className).toContain("border-foreground");
    });

    it("re-populates with same project data after close and reopen (discards user edits)", async () => {
      const user = userEvent.setup();
      const editingProject = makeEditingProject({ name: "Original" });

      const { rerender } = render(
        <CreateProjectDialog
          {...defaultProps}
          isOpen={true}
          editingProject={editingProject}
        />,
      );

      // User modifies the name
      const nameInput = screen.getByPlaceholderText("My Project");
      await user.clear(nameInput);
      await user.type(nameInput, "User Typed");
      expect(nameInput).toHaveValue("User Typed");

      // Close the dialog
      rerender(
        <CreateProjectDialog
          {...defaultProps}
          isOpen={false}
          editingProject={editingProject}
        />,
      );

      // Reopen with the same project
      rerender(
        <CreateProjectDialog
          {...defaultProps}
          isOpen={true}
          editingProject={makeEditingProject({ name: "Original" })}
        />,
      );

      // Fields should be re-populated from editingProject, not the user's edits
      expect(screen.getByPlaceholderText("My Project")).toHaveValue("Original");
    });
  });

  // ── Create mode (no editingProject) ──────────────────────────────────

  describe("create mode", () => {
    it("uses initialWorkingDir to derive project name", () => {
      render(
        <CreateProjectDialog
          {...defaultProps}
          isOpen={true}
          initialWorkingDir="/home/user/my-repo"
        />,
      );

      const nameInput = screen.getByPlaceholderText("My Project");
      expect(nameInput).toHaveValue("my-repo");
    });

    it("does not reset create-mode fields on re-render while open", async () => {
      const user = userEvent.setup();

      const { rerender } = render(
        <CreateProjectDialog
          {...defaultProps}
          isOpen={true}
          initialWorkingDir="/home/user/my-repo"
        />,
      );

      const nameInput = screen.getByPlaceholderText("My Project");
      expect(nameInput).toHaveValue("my-repo");

      await user.clear(nameInput);
      await user.type(nameInput, "Custom Name");
      expect(nameInput).toHaveValue("Custom Name");

      // Re-render (parent re-renders) - dialog stays open
      rerender(
        <CreateProjectDialog
          {...defaultProps}
          isOpen={true}
          initialWorkingDir="/home/user/my-repo"
        />,
      );

      // User's text should be preserved
      expect(nameInput).toHaveValue("Custom Name");
    });
  });
});
