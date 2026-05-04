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
    renderAdapter({ open: true });
    expect(screen.getByText(/created \/project\/output\.md/i)).toBeVisible();
  });

  it("renders structured content when present", () => {
    renderAdapter({
      open: true,
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
