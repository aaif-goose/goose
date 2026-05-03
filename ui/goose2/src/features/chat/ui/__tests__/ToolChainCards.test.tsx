import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ToolChainCards } from "../ToolChainCards";
import type { ToolChainItem } from "@/features/chat/lib/toolChainGrouping";

vi.mock("@/features/chat/hooks/ArtifactPolicyContext", () => ({
  useArtifactPolicyContext: () => ({
    resolveToolCardDisplay: () => ({
      role: "none",
      primaryCandidate: null,
      secondaryCandidates: [],
    }),
    resolveMarkdownHref: () => null,
    pathExists: vi.fn().mockResolvedValue(false),
    openResolvedPath: vi.fn().mockResolvedValue(undefined),
  }),
}));

let nextId = 0;

function pair(
  name: string,
  options: {
    isError?: boolean;
    status?: ToolChainItem["request"] extends infer R
      ? R extends { status: infer S }
        ? S
        : never
      : never;
    completed?: boolean;
  } = {},
): ToolChainItem {
  const id = `tool-${++nextId}`;
  const completed = options.completed !== false;
  return {
    key: id,
    request: {
      type: "toolRequest",
      id,
      name,
      arguments: {},
      status: options.status ?? "completed",
    },
    response: completed
      ? {
          type: "toolResponse",
          id,
          name,
          result: "ok",
          isError: options.isError ?? false,
        }
      : undefined,
  };
}

describe("ToolChainCards", () => {
  it("renders without a parent header for a single tool item", () => {
    render(<ToolChainCards toolItems={[pair("Read · src/a.ts")]} />);
    expect(
      screen.queryByRole("button", { name: /reviewing files|step/i }),
    ).not.toBeInTheDocument();
  });

  it("renders a deterministic chain header for multi-tool chains", () => {
    render(
      <ToolChainCards
        toolItems={[pair("Shell · npm test"), pair("Shell · npm run build")]}
      />,
    );
    expect(
      screen.getByRole("button", { name: /running commands.*2 step/i }),
    ).toBeInTheDocument();
  });

  it("uses the active label while any step is still in progress", () => {
    render(
      <ToolChainCards
        toolItems={[
          pair("Shell · npm test", { completed: true }),
          pair("Shell · npm build", {
            status: "executing",
            completed: false,
          }),
        ]}
      />,
    );
    expect(
      screen.getByRole("button", { name: /working through 2 steps/i }),
    ).toBeInTheDocument();
  });

  it("collapses and re-expands the chain when the header is clicked", async () => {
    const user = userEvent.setup();
    render(
      <ToolChainCards
        toolItems={[pair("Edit · src/a.ts"), pair("Edit · src/b.ts")]}
      />,
    );
    const header = screen.getByRole("button", {
      name: /updating files.*2 steps/i,
    });
    expect(header).toHaveAttribute("aria-expanded", "true");
    await user.click(header);
    expect(header).toHaveAttribute("aria-expanded", "false");
  });

  it("surfaces error status as a data attribute on the chain wrapper", () => {
    const { container } = render(
      <ToolChainCards
        toolItems={[
          pair("Shell · npm test"),
          pair("Shell · npm build", { isError: true }),
        ]}
      />,
    );
    const wrapper = container.querySelector('[data-role="tool-chain-card"]');
    expect(wrapper?.getAttribute("data-status")).toBe("error");
  });
});
