import { render, screen } from "@testing-library/react";
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
      status="completed"
      result="Created /project/output.md"
      {...overrides}
    />,
  );
}

// ── tests ────────────────────────────────────────────────────────────

describe("ToolCallAdapter — ArtifactActions", () => {
  it('renders "Open file" button when primary candidate exists', () => {
    const primary = makeCandidate();
    mockResolveToolCardDisplay.mockReturnValue({
      role: "primary_host",
      primaryCandidate: primary,
      secondaryCandidates: [],
    });

    renderAdapter();

    expect(screen.getByRole("button", { name: /open file/i })).toBeEnabled();
    expect(screen.getByText(primary.rawPath)).toBeInTheDocument();
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
    expect(screen.queryByText(secondary.rawPath)).not.toBeInTheDocument();

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
