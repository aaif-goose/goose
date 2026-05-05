import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi, beforeEach } from "vitest";
import { ToolCallAdapter } from "../ToolCallAdapter";

const mockOpenResolvedPath = vi.fn<(path: string) => Promise<void>>();

vi.mock("@/features/chat/hooks/ArtifactPolicyContext", () => ({
  useArtifactPolicyContext: () => ({
    resolveMarkdownHref: () => null,
    pathExists: async () => false,
    openResolvedPath: mockOpenResolvedPath,
    getAllSessionArtifacts: () => [],
  }),
}));

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

describe("ToolCallAdapter — output", () => {
  beforeEach(() => {
    mockOpenResolvedPath.mockReset();
  });

  it("shows content and structured content together in the parent tool accordion", () => {
    renderAdapter({
      open: true,
      result: JSON.stringify({
        restaurants: [{ name: "Content Coffee" }],
      }),
      structuredContent: {
        restaurants: [{ name: "Structured Coffee" }],
      },
    });

    expect(screen.getByText("Content")).toBeInTheDocument();
    expect(screen.getByText("Structured content")).toBeInTheDocument();
    expect(screen.getByText(/Content Coffee/)).toBeInTheDocument();
    expect(screen.getByText(/Structured Coffee/)).toBeInTheDocument();
    expect(
      screen.getByText(/"restaurants":\[\{"name":"Content Coffee"\}\]/),
    ).toBeInTheDocument();
    expect(
      screen.queryByText(/structured output .*lines/i),
    ).not.toBeInTheDocument();
  });

  it("shows falsy primitive structured content", () => {
    renderAdapter({
      open: true,
      result: "Completed",
      structuredContent: false,
    });

    expect(screen.getByText("Structured content")).toBeInTheDocument();
    expect(screen.getByText("false")).toBeInTheDocument();
  });
});

describe("ToolCallAdapter — ArtifactActions", () => {
  beforeEach(() => {
    mockOpenResolvedPath.mockReset();
  });

  it('renders "Open file" button for reported ACP locations', () => {
    renderAdapter({
      locations: [{ path: "/Users/test/project/output.md" }],
    });

    expect(screen.getByRole("button", { name: /open file/i })).toBeEnabled();
    expect(
      screen.getByText("/Users/test/project/output.md"),
    ).toBeInTheDocument();
  });

  it("does NOT render artifact actions when no locations are reported", () => {
    renderAdapter();

    expect(
      screen.queryByRole("button", { name: /open file/i }),
    ).not.toBeInTheDocument();
  });

  it('shows "More outputs" toggle for secondary locations', async () => {
    const user = userEvent.setup();
    renderAdapter({
      locations: [
        { path: "/Users/test/project/output.md" },
        { path: "/Users/test/project/notes.md" },
      ],
    });

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

  it("opens reported location when primary button is clicked", async () => {
    const user = userEvent.setup();
    mockOpenResolvedPath.mockResolvedValue(undefined);

    renderAdapter({
      locations: [{ path: "/Users/test/project/output.md" }],
    });
    await user.click(screen.getByRole("button", { name: /open file/i }));

    expect(mockOpenResolvedPath).toHaveBeenCalledWith(
      "/Users/test/project/output.md",
    );
  });

  it("shows opener errors", async () => {
    const user = userEvent.setup();
    mockOpenResolvedPath.mockRejectedValue(
      new Error("File not found: /Users/test/project/output.md"),
    );

    renderAdapter({
      locations: [{ path: "/Users/test/project/output.md" }],
    });
    await user.click(screen.getByRole("button", { name: /open file/i }));

    expect(
      await screen.findByText("File not found: /Users/test/project/output.md"),
    ).toBeInTheDocument();
  });
});
