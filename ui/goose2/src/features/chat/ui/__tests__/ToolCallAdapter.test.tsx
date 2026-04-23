import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { ToolCardDisplay } from "@/features/chat/hooks/ArtifactPolicyContext";
import type { ArtifactPathCandidate } from "@/features/chat/lib/artifactPathPolicy";
import { ToolCallAdapter } from "../ToolCallAdapter";

// ── mocks ────────────────────────────────────────────────────────────

const mockResolveToolCardDisplay =
  vi.fn<
    (
      args: Record<string, unknown>,
      name: string,
      result?: string,
    ) => ToolCardDisplay
  >();
const mockPathExists = vi.fn<(path: string) => Promise<boolean>>();
const mockOpenResolvedPath = vi.fn<(path: string) => Promise<void>>();

vi.mock("@/features/chat/hooks/ArtifactPolicyContext", () => ({
  useArtifactPolicyContext: () => ({
    resolveToolCardDisplay: mockResolveToolCardDisplay,
    resolveMarkdownHref: () => null,
    pathExists: mockPathExists,
    openResolvedPath: mockOpenResolvedPath,
  }),
}));

// ── helpers ──────────────────────────────────────────────────────────

const EMPTY_DISPLAY: ToolCardDisplay = {
  role: "none",
  primaryCandidate: null,
  secondaryCandidates: [],
};

function makeCandidate(
  overrides: Partial<ArtifactPathCandidate> = {},
): ArtifactPathCandidate {
  return {
    id: "c-1",
    rawPath: "/project/output.md",
    resolvedPath: "/Users/test/project/output.md",
    source: "arg_key",
    confidence: "high",
    kind: "file",
    allowed: true,
    blockedReason: null,
    toolCallId: "tool-1",
    toolName: "write_file",
    toolCallIndex: 0,
    appearanceIndex: 0,
    ...overrides,
  };
}

function renderAdapter(
  overrides: Partial<Parameters<typeof ToolCallAdapter>[0]> = {},
) {
  return render(
    <ToolCallAdapter
      name="write_file"
      arguments={{ path: "/project/output.md" }}
      kind="edit"
      status="completed"
      result="Created /project/output.md"
      open
      {...overrides}
    />,
  );
}

// ── tests ────────────────────────────────────────────────────────────

describe("ToolCallAdapter — ArtifactActions", () => {
  it("renders a deterministic input summary and reveals raw input on demand", async () => {
    const user = userEvent.setup();
    mockResolveToolCardDisplay.mockReturnValue(EMPTY_DISPLAY);

    renderAdapter({
      name: "read_file",
      kind: "read",
      arguments: { path: "/project/src/main.ts", line: 12 },
      result: "file contents",
    });

    expect(screen.getByText("Path")).toBeInTheDocument();
    expect(screen.getByText("main.ts")).toBeInTheDocument();
    expect(screen.getByText("Line")).toBeInTheDocument();
    expect(screen.getByText("12")).toBeInTheDocument();
    expect(screen.queryByText("/project/src/main.ts")).not.toBeInTheDocument();
    expect(
      screen.queryByText('"path": "/project/src/main.ts"'),
    ).not.toBeInTheDocument();
    expect(screen.queryByText("Parameters")).not.toBeInTheDocument();
    expect(screen.queryByText("Raw input")).not.toBeInTheDocument();

    const rawInputTrigger = screen.getByText("main.ts").closest("button");

    expect(rawInputTrigger).toBeTruthy();

    if (!rawInputTrigger) {
      throw new Error("Expected raw input trigger");
    }

    await user.click(rawInputTrigger);

    const rawInputPanel = rawInputTrigger
      ?.closest('[data-slot="collapsible"]')
      ?.querySelector("pre");

    expect(rawInputPanel).toHaveTextContent('"path": "/project/src/main.ts"');
  });

  it("opens the file from the header filename without expanding the accordion", async () => {
    const user = userEvent.setup();
    mockResolveToolCardDisplay.mockReturnValue(EMPTY_DISPLAY);
    mockOpenResolvedPath.mockResolvedValue(undefined);

    renderAdapter({
      name: "Edit main.swift",
      kind: "edit",
      arguments: { path: "/project/Sources/main.swift", line: 1 },
      result: "Updated /project/Sources/main.swift",
      open: undefined,
    });

    await user.click(screen.getByRole("button", { name: /open main\.swift/i }));

    expect(mockOpenResolvedPath).toHaveBeenCalledWith(
      "/project/Sources/main.swift",
    );
    expect(screen.queryByText("Path")).not.toBeInTheDocument();
  });

  it("opens the accordion when clicking the non-file title text", async () => {
    const user = userEvent.setup();
    mockResolveToolCardDisplay.mockReturnValue(EMPTY_DISPLAY);
    mockOpenResolvedPath.mockReset();
    mockOpenResolvedPath.mockResolvedValue(undefined);

    const { container } = renderAdapter({
      name: "Edit main.swift",
      kind: "edit",
      arguments: { path: "/project/Sources/main.swift", line: 1 },
      result: "Updated /project/Sources/main.swift",
      open: undefined,
    });

    const titlePrefix = container.querySelector("[data-tool-title-prefix]");
    expect(titlePrefix).toBeTruthy();

    if (!titlePrefix) {
      throw new Error("Expected non-file title text");
    }

    await user.click(titlePrefix);

    expect(mockOpenResolvedPath).not.toHaveBeenCalled();
    expect(screen.getByText("Path")).toBeInTheDocument();
  });

  it("renders command input summaries with a clamped preview and expanded bash highlighting", async () => {
    const user = userEvent.setup();
    mockResolveToolCardDisplay.mockReturnValue(EMPTY_DISPLAY);

    const { container } = renderAdapter({
      name: "shell",
      kind: "execute",
      arguments: {
        command: "cat /project/package.json",
        cwd: "/project",
      },
      result: "package contents",
    });

    expect(screen.getByText("Command")).toBeInTheDocument();
    expect(screen.getByText("cat /project/package.json")).toBeInTheDocument();
    expect(screen.getByText("Working directory")).toBeInTheDocument();
    expect(screen.getByText("/project")).toBeInTheDocument();
    expect(screen.getByText("package contents")).toBeInTheDocument();
    expect(screen.queryByText("Result")).not.toBeInTheDocument();
    const commandPreview = container.querySelector(
      "[data-tool-command-preview]",
    );
    expect(commandPreview).toBeTruthy();
    expect(commandPreview?.className).toContain("[&_pre]:line-clamp-3");
    expect(container.querySelector('[data-language="bash"]')).toBeTruthy();
    expect(container.querySelector('[data-language="json"]')).toBeFalsy();

    await user.click(screen.getByText("cat /project/package.json"));

    const commandBlock = container.querySelector('[data-language="bash"]');
    expect(commandBlock).toBeTruthy();
    expect(commandBlock).toHaveTextContent("cat /project/package.json");
    expect(container.querySelector('[data-language="json"]')).toBeFalsy();
  });

  it("renders single-file responses without a duplicate files section", () => {
    mockResolveToolCardDisplay.mockReturnValue(EMPTY_DISPLAY);

    const view = renderAdapter({
      name: "read_file",
      kind: "read",
      arguments: { path: "/project/src/renderer.js" },
      locations: [{ path: "/project/src/renderer.js", line: 42 }],
      rawOutput: "rendered raw output",
      result: "flattened result",
    });

    expect(screen.getByText("Path")).toBeInTheDocument();
    expect(screen.getByText("renderer.js")).toBeInTheDocument();
    expect(screen.getByText("Line")).toBeInTheDocument();
    expect(screen.getByText("42")).toBeInTheDocument();
    expect(screen.getByText("flattened result")).toBeInTheDocument();
    expect(screen.queryByText("Result")).not.toBeInTheDocument();
    expect(screen.queryByText("Raw result")).not.toBeInTheDocument();
    expect(screen.queryByText("Files")).not.toBeInTheDocument();
    expect(screen.queryByText("renderer.js:42")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /open file/i }),
    ).not.toBeInTheDocument();
    expect(screen.queryByText(/more outputs/i)).not.toBeInTheDocument();
    expect(view.container.querySelector('[data-language="json"]')).toBeFalsy();
  });

  it("renders multi-location fallbacks as inline file pills", async () => {
    const user = userEvent.setup();
    mockResolveToolCardDisplay.mockReturnValue(EMPTY_DISPLAY);
    mockPathExists.mockResolvedValue(true);
    mockOpenResolvedPath.mockResolvedValue(undefined);

    renderAdapter({
      name: "read_file",
      kind: "read",
      arguments: { path: "/project/src/renderer.js", line: 42 },
      locations: [
        { path: "/project/src/renderer.js", line: 42 },
        { path: "/project/src/index.js", line: 12 },
      ],
      result: "flattened result",
    });

    expect(screen.getByText("Files")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /renderer\.js:42/i }),
    ).toHaveClass("rounded-full");
    expect(screen.getByRole("button", { name: /index\.js:12/i })).toHaveClass(
      "rounded-full",
    );
    expect(
      screen.queryByText("/project/src/index.js:12"),
    ).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /index\.js:12/i }));

    expect(mockOpenResolvedPath).toHaveBeenCalledWith("/project/src/index.js");
  });

  it('renders "Open file" button when primary candidate exists', () => {
    const primary = makeCandidate();
    mockResolveToolCardDisplay.mockReturnValue({
      role: "primary_host",
      primaryCandidate: primary,
      secondaryCandidates: [],
    });

    renderAdapter();

    const openFileButton = screen.getByRole("button", { name: /open file/i });
    expect(openFileButton).toBeEnabled();
    expect(openFileButton).toHaveTextContent(primary.rawPath ?? "");
    expect(screen.getByText("output.md")).toBeInTheDocument();
  });

  it("does NOT render artifact actions when display role is none", () => {
    mockResolveToolCardDisplay.mockReturnValue(EMPTY_DISPLAY);

    renderAdapter();

    expect(
      screen.queryByRole("button", { name: /open file/i }),
    ).not.toBeInTheDocument();
  });

  it('shows "More outputs" toggle for secondary candidates', async () => {
    const user = userEvent.setup();
    const primary = makeCandidate();
    const secondary = makeCandidate({
      id: "c-2",
      rawPath: "/project/notes.md",
      resolvedPath: "/Users/test/project/notes.md",
    });
    mockResolveToolCardDisplay.mockReturnValue({
      role: "primary_host",
      primaryCandidate: primary,
      secondaryCandidates: [secondary],
    });

    renderAdapter();

    const toggle = screen.getByText(/more outputs/i);
    expect(toggle).toBeInTheDocument();

    // Secondary button not visible initially
    expect(
      within(toggle.closest("div") ?? document.body).queryByText(
        secondary.rawPath,
      ),
    ).not.toBeInTheDocument();

    await user.click(toggle);

    // After expanding, secondary candidate is visible
    expect(screen.getByText(secondary.rawPath)).toBeInTheDocument();
  });

  it("disables button and shows blocked reason for disallowed primary candidate", () => {
    const blocked = makeCandidate({
      allowed: false,
      blockedReason: "Path is outside allowed project/artifacts roots.",
    });
    mockResolveToolCardDisplay.mockReturnValue({
      role: "primary_host",
      primaryCandidate: blocked,
      secondaryCandidates: [],
    });

    renderAdapter();

    expect(screen.getByRole("button", { name: /open file/i })).toBeDisabled();
    expect(
      screen.getByText("Path is outside allowed project/artifacts roots."),
    ).toBeInTheDocument();
  });

  it("shows blocked reason for disallowed secondary candidates", async () => {
    const user = userEvent.setup();
    const primary = makeCandidate();
    const blockedSecondary = makeCandidate({
      id: "c-2",
      rawPath: "/outside/secret.md",
      resolvedPath: "/Users/test/outside/secret.md",
      allowed: false,
      blockedReason: "Path is outside allowed project/artifacts roots.",
    });
    mockResolveToolCardDisplay.mockReturnValue({
      role: "primary_host",
      primaryCandidate: primary,
      secondaryCandidates: [blockedSecondary],
    });

    renderAdapter();
    await user.click(screen.getByText(/more outputs/i));

    const secondaryBtn = screen.getByTitle(blockedSecondary.resolvedPath);
    expect(secondaryBtn).toBeDisabled();
    expect(
      screen.getByText("Path is outside allowed project/artifacts roots."),
    ).toBeInTheDocument();
  });

  it('does not show "detected" label for low-confidence primary candidate', () => {
    const lowConf = makeCandidate({ confidence: "low" });
    mockResolveToolCardDisplay.mockReturnValue({
      role: "primary_host",
      primaryCandidate: lowConf,
      secondaryCandidates: [],
    });

    renderAdapter();

    expect(screen.queryByText("detected")).not.toBeInTheDocument();
  });

  it('does NOT show "detected" label for high-confidence candidate', () => {
    const highConf = makeCandidate({ confidence: "high" });
    mockResolveToolCardDisplay.mockReturnValue({
      role: "primary_host",
      primaryCandidate: highConf,
      secondaryCandidates: [],
    });

    renderAdapter();

    expect(screen.queryByText("detected")).not.toBeInTheDocument();
  });

  it('does not show "detected" label for low-confidence secondary candidate', async () => {
    const user = userEvent.setup();
    const primary = makeCandidate();
    const lowConfSecondary = makeCandidate({
      id: "c-2",
      rawPath: "/project/maybe.md",
      resolvedPath: "/Users/test/project/maybe.md",
      confidence: "low",
    });
    mockResolveToolCardDisplay.mockReturnValue({
      role: "primary_host",
      primaryCandidate: primary,
      secondaryCandidates: [lowConfSecondary],
    });

    renderAdapter();
    await user.click(screen.getByText(/more outputs/i));

    expect(screen.queryByText("detected")).not.toBeInTheDocument();
  });

  it("opens file when primary button is clicked", async () => {
    const user = userEvent.setup();
    const primary = makeCandidate();
    mockResolveToolCardDisplay.mockReturnValue({
      role: "primary_host",
      primaryCandidate: primary,
      secondaryCandidates: [],
    });
    mockPathExists.mockResolvedValue(true);
    mockOpenResolvedPath.mockResolvedValue(undefined);

    renderAdapter();
    await user.click(screen.getByRole("button", { name: /open file/i }));

    expect(mockOpenResolvedPath).toHaveBeenCalledWith(primary.resolvedPath);
  });

  it("shows file-not-found error when path does not exist", async () => {
    const user = userEvent.setup();
    const primary = makeCandidate();
    mockResolveToolCardDisplay.mockReturnValue({
      role: "primary_host",
      primaryCandidate: primary,
      secondaryCandidates: [],
    });
    mockPathExists.mockResolvedValue(false);

    renderAdapter();
    await user.click(screen.getByRole("button", { name: /open file/i }));

    expect(
      await screen.findByText(`File not found: ${primary.resolvedPath}`),
    ).toBeInTheDocument();
  });
});
