import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi, beforeEach } from "vitest";
import type { ArtifactPathCandidate } from "@/features/chat/lib/artifactPathPolicy";

// ── mocks ────────────────────────────────────────────────────────────

const mockResolveMarkdownHref =
  vi.fn<(href: string) => ArtifactPathCandidate | null>();
const mockOpenResolvedPath = vi.fn<(path: string) => Promise<void>>();

vi.mock("@/features/chat/hooks/ArtifactPolicyContext", () => ({
  useArtifactPolicyContext: () => ({
    resolveToolCardDisplay: () => ({
      role: "none",
      primaryCandidate: null,
      secondaryCandidates: [],
    }),
    resolveMarkdownHref: mockResolveMarkdownHref,
    pathExists: async () => false,
    openResolvedPath: mockOpenResolvedPath,
  }),
}));

import { useArtifactLinkHandler } from "../useArtifactLinkHandler";

// ── helpers ──────────────────────────────────────────────────────────

function makeCandidate(
  overrides: Partial<ArtifactPathCandidate> = {},
): ArtifactPathCandidate {
  return {
    id: "md-1",
    rawPath: "/project/report.md",
    resolvedPath: "/Users/test/project/report.md",
    source: "arg_key",
    confidence: "high",
    kind: "file",
    allowed: true,
    blockedReason: null,
    toolCallId: null,
    toolName: null,
    toolCallIndex: 0,
    appearanceIndex: 0,
    ...overrides,
  };
}

/** Renders a container with the click handler and an anchor link inside. */
function Harness({ href, label }: { href: string; label: string }) {
  const { handleContentClick, pathNotice } = useArtifactLinkHandler();
  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents: test harness only
    // biome-ignore lint/a11y/noStaticElementInteractions: test harness only
    <div onClick={handleContentClick}>
      <a href={href}>{label}</a>
      {pathNotice && <p data-testid="notice">{pathNotice}</p>}
    </div>
  );
}

/** Renders a container with a non-link element. */
function HarnessNoLink() {
  const { handleContentClick, pathNotice } = useArtifactLinkHandler();
  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents: test harness only
    // biome-ignore lint/a11y/noStaticElementInteractions: test harness only
    <div onClick={handleContentClick}>
      <span data-testid="plain">just text</span>
      {pathNotice && <p data-testid="notice">{pathNotice}</p>}
    </div>
  );
}

// ── tests ────────────────────────────────────────────────────────────

describe("useArtifactLinkHandler", () => {
  beforeEach(() => {
    mockResolveMarkdownHref.mockReset();
    mockOpenResolvedPath.mockReset();
  });

  it("calls resolveMarkdownHref and openResolvedPath for allowed local links", async () => {
    const user = userEvent.setup();
    const candidate = makeCandidate();
    mockResolveMarkdownHref.mockReturnValue(candidate);
    mockOpenResolvedPath.mockResolvedValue(undefined);

    render(<Harness href="/project/report.md" label="Report" />);
    await user.click(screen.getByText("Report"));

    expect(mockResolveMarkdownHref).toHaveBeenCalledWith("/project/report.md");
    expect(mockOpenResolvedPath).toHaveBeenCalledWith(candidate.resolvedPath);
  });

  it("shows blocked notice for disallowed paths", async () => {
    const user = userEvent.setup();
    const blocked = makeCandidate({
      allowed: false,
      blockedReason: "Path is outside allowed project/artifacts roots.",
    });
    mockResolveMarkdownHref.mockReturnValue(blocked);

    render(<Harness href="/secret/data.md" label="Secret" />);
    await user.click(screen.getByText("Secret"));

    expect(mockOpenResolvedPath).not.toHaveBeenCalled();
    expect(screen.getByTestId("notice")).toHaveTextContent(
      "Path is outside allowed project/artifacts roots.",
    );
  });

  it("does not intercept external URLs (defers to MarkdownLink's LinkSafetyModal)", async () => {
    const user = userEvent.setup();

    render(<Harness href="https://example.com" label="External" />);
    await user.click(screen.getByText("External"));

    expect(mockResolveMarkdownHref).not.toHaveBeenCalled();
    expect(mockOpenResolvedPath).not.toHaveBeenCalled();
  });

  it("ignores clicks on non-link elements", async () => {
    const user = userEvent.setup();

    render(<HarnessNoLink />);
    await user.click(screen.getByTestId("plain"));

    expect(mockResolveMarkdownHref).not.toHaveBeenCalled();
    expect(mockOpenResolvedPath).not.toHaveBeenCalled();
  });

  it("shows default blocked reason when blockedReason is null", async () => {
    const user = userEvent.setup();
    const blocked = makeCandidate({
      allowed: false,
      blockedReason: null,
    });
    mockResolveMarkdownHref.mockReturnValue(blocked);

    render(<Harness href="/outside/file.md" label="Blocked" />);
    await user.click(screen.getByText("Blocked"));

    expect(screen.getByTestId("notice")).toHaveTextContent(
      "Path is outside allowed project/artifacts roots.",
    );
  });
});
