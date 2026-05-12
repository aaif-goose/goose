import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ArtifactLinkCandidate } from "@/features/chat/hooks/ArtifactPolicyContext";
import type { ToolCallLocation } from "@/shared/types/messages";
import { ToolCallAdapter } from "../ToolCallAdapter";

const mockResolveMarkdownHref =
  vi.fn<(href: string) => ArtifactLinkCandidate | null>();
const mockPathExists = vi.fn<(path: string) => Promise<boolean>>();
const mockOpenResolvedPath = vi.fn<(path: string) => Promise<void>>();

vi.mock("@/features/chat/hooks/ArtifactPolicyContext", () => ({
  useArtifactPolicyContext: () => ({
    resolveMarkdownHref: mockResolveMarkdownHref,
    pathExists: mockPathExists,
    openResolvedPath: mockOpenResolvedPath,
    getAllSessionArtifacts: () => [],
  }),
}));

beforeEach(() => {
  mockResolveMarkdownHref.mockReturnValue(null);
});

afterEach(() => {
  vi.clearAllMocks();
});

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

describe("ToolCallAdapter — ArtifactActions", () => {
  it('renders "Open file" button when a location is provided', () => {
    const locations: ToolCallLocation[] = [
      { path: "/Users/test/project/output.md" },
    ];

    renderAdapter({ locations });

    expect(screen.getByRole("button", { name: /open file/i })).toBeEnabled();
    expect(
      screen.getByText("/Users/test/project/output.md"),
    ).toBeInTheDocument();
  });

  it("does NOT render artifact actions when no locations are provided", () => {
    renderAdapter();

    expect(
      screen.queryByRole("button", { name: /open file/i }),
    ).not.toBeInTheDocument();
  });

  it('shows "More outputs" toggle when there are multiple locations', async () => {
    const user = userEvent.setup();
    const locations: ToolCallLocation[] = [
      { path: "/Users/test/project/output.md" },
      { path: "/Users/test/project/notes.md" },
    ];

    renderAdapter({ locations });

    const toggle = screen.getByText(/more outputs/i);
    expect(toggle).toBeInTheDocument();

    expect(
      screen.queryByText("/Users/test/project/notes.md"),
    ).not.toBeInTheDocument();

    await user.click(toggle);

    expect(
      screen.getByText("/Users/test/project/notes.md"),
    ).toBeInTheDocument();
  });

  it("invokes openResolvedPath when an artifact button is clicked", async () => {
    const user = userEvent.setup();
    mockOpenResolvedPath.mockResolvedValue(undefined);
    const locations: ToolCallLocation[] = [
      { path: "/Users/test/project/output.md" },
    ];

    renderAdapter({ locations });

    await user.click(screen.getByRole("button", { name: /open file/i }));

    expect(mockOpenResolvedPath).toHaveBeenCalledWith(
      "/Users/test/project/output.md",
    );
  });
});

describe("ToolCallAdapter — expanded body", () => {
  it("renders the tool name and status badge in the header", () => {
    renderAdapter();
    expect(
      screen.getByRole("button", { name: /write_file/i }),
    ).toBeInTheDocument();
  });

  it("shows the text result when expanded", () => {
    renderAdapter({ open: true, structuredContent: undefined });
    expect(screen.getByText(/created \/project\/output\.md/i)).toBeVisible();
  });

  it("renders structured content when present without a text result", () => {
    renderAdapter({
      open: true,
      result: undefined,
      structuredContent: { kind: "summary", count: 3 },
    });

    expect(screen.getByText(/"kind"/)).toBeInTheDocument();
    expect(screen.getByText(/"summary"/)).toBeInTheDocument();
  });

  it("renders the error result when isError is true", () => {
    renderAdapter({ open: true, isError: true, result: "Boom" });
    expect(screen.getByText("Boom")).toBeInTheDocument();
  });
});

describe("ToolCallAdapter — text + structured de-dupe matrix", () => {
  it("hides redundant text when result is a stringified copy of structured", () => {
    const structured = { kind: "summary", count: 3 };
    renderAdapter({
      open: true,
      arguments: {},
      result: JSON.stringify(structured),
      structuredContent: structured,
    });

    // The structured payload renders exactly once, not twice.
    const summaryMatches = screen.getAllByText(/"summary"/);
    expect(summaryMatches).toHaveLength(1);
  });

  it("hoists short single-line text into the header when both differ", () => {
    renderAdapter({
      open: true,
      arguments: {},
      result: "Found 3 matches",
      structuredContent: { matches: 3 },
    });

    // The hoisted text renders inside the header subtitle slot.
    const hoisted = document.querySelector("[data-tool-title-hoisted]");
    expect(hoisted).not.toBeNull();
    expect(hoisted?.textContent).toContain("Found 3 matches");

    // Body shows the structured payload but does NOT duplicate the hoisted
    // text — i.e. "Found 3 matches" appears exactly once across the card.
    const allMatches = screen.getAllByText(/Found 3 matches/);
    expect(allMatches).toHaveLength(1);
    expect(screen.getByText(/"matches"/)).toBeInTheDocument();
  });

  it("renders both text and structured in the body when text is multi-line", () => {
    renderAdapter({
      open: true,
      arguments: {},
      result: "line one\nline two\nline three",
      structuredContent: { matches: 3 },
    });

    // No header hoisting for multi-line text.
    expect(document.querySelector("[data-tool-title-hoisted]")).toBeNull();

    // Both blocks render in the body.
    expect(screen.getByText(/line one/)).toBeInTheDocument();
    expect(screen.getByText(/"matches"/)).toBeInTheDocument();
  });

  it("does not hoist text when path-based header hoisting takes precedence", () => {
    // Tool name contains the basename → path-based hoisting activates.
    renderAdapter({
      open: true,
      name: "Write file output.md",
      arguments: { path: "/project/output.md" },
      result: "Wrote file",
      structuredContent: { bytes: 42 },
    });

    // Path-based hoisting wins; result text stays in the body.
    expect(document.querySelector("[data-tool-title-hoisted]")).toBeNull();
    expect(screen.getByText(/wrote file/i)).toBeInTheDocument();
    expect(screen.getByText(/"bytes"/)).toBeInTheDocument();
  });
});
