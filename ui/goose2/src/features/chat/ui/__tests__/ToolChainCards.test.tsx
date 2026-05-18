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
            status: "in_progress",
            completed: false,
          }),
        ]}
      />,
    );
    expect(
      screen.getByRole("button", { name: /working through 2 steps/i }),
    ).toBeInTheDocument();
  });

  it("collapses and re-expands an active chain when the header is clicked", async () => {
    const user = userEvent.setup();
    render(
      <ToolChainCards
        toolItems={[
          pair("Edit · src/a.ts"),
          pair("Edit · src/b.ts", {
            status: "in_progress",
            completed: false,
          }),
        ]}
      />,
    );
    const header = screen.getByRole("button", {
      name: /working through 2 steps/i,
    });
    expect(header).toHaveAttribute("aria-expanded", "true");
    await user.click(header);
    expect(header).toHaveAttribute("aria-expanded", "false");
  });

  it("starts collapsed when the chain mounts already complete (replay)", async () => {
    const user = userEvent.setup();
    render(
      <ToolChainCards
        toolItems={[pair("Edit · src/a.ts"), pair("Edit · src/b.ts")]}
      />,
    );
    const header = screen.getByRole("button", {
      name: /updating files.*2 steps/i,
    });
    expect(header).toHaveAttribute("aria-expanded", "false");
    await user.click(header);
    expect(header).toHaveAttribute("aria-expanded", "true");
  });

  it("auto-collapses a live chain once every step has completed", () => {
    const a = pair("Edit · src/a.ts");
    const bRequest = pair("Edit · src/b.ts", {
      status: "in_progress",
      completed: false,
    });
    const { rerender } = render(<ToolChainCards toolItems={[a, bRequest]} />);
    const activeHeader = screen.getByRole("button", {
      name: /working through 2 steps/i,
    });
    expect(activeHeader).toHaveAttribute("aria-expanded", "true");

    // Same chain identity, but the second step now has a response — i.e. the
    // chain has just completed in realtime.
    const bComplete: typeof bRequest = {
      ...bRequest,
      request: bRequest.request
        ? { ...bRequest.request, status: "completed" }
        : bRequest.request,
      response: {
        type: "toolResponse",
        id: bRequest.request?.id ?? "tool-x",
        name: "Edit · src/b.ts",
        result: "ok",
        isError: false,
      },
    };
    rerender(<ToolChainCards toolItems={[a, bComplete]} />);

    const completedHeader = screen.getByRole("button", {
      name: /updating files.*2 steps/i,
    });
    expect(completedHeader).toHaveAttribute("aria-expanded", "false");
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
    expect(wrapper?.getAttribute("data-status")).toBe("failed");
  });

  it("renders a step rail row for each child inside a chain", () => {
    const { container } = render(
      <ToolChainCards
        toolItems={[
          pair("Edit · src/a.ts"),
          pair("Edit · src/b.ts"),
          pair("Edit · src/c.ts", {
            status: "in_progress",
            completed: false,
          }),
        ]}
      />,
    );
    const rows = container.querySelectorAll('[data-role="tool-chain-step"]');
    expect(rows).toHaveLength(3);
  });

  it("does not wrap a single tool call in a rail row", () => {
    const { container } = render(
      <ToolChainCards toolItems={[pair("Read · src/a.ts")]} />,
    );
    const rows = container.querySelectorAll('[data-role="tool-chain-step"]');
    expect(rows).toHaveLength(0);
  });

  it("renders a left caret button on a single tool call that toggles its open state", async () => {
    const user = userEvent.setup();
    const { container } = render(
      <ToolChainCards toolItems={[pair("Read · src/a.ts")]} />,
    );
    const wrapper = container.querySelector('[data-role="tool-single"]');
    expect(wrapper).not.toBeNull();

    const caret = wrapper?.querySelector(
      ":scope > button",
    ) as HTMLButtonElement;
    expect(caret).toBeTruthy();
    expect(caret).toHaveAttribute("aria-expanded", "false");
    await user.click(caret);
    expect(caret).toHaveAttribute("aria-expanded", "true");
  });

  it("hides the trailing right-side chevron on a single tool call", () => {
    const { container } = render(
      <ToolChainCards toolItems={[pair("Read · src/a.ts")]} />,
    );
    // The shared ToolHeader's trailing chevron is a CollapsibleTrigger
    // styled with the group-data-[state=closed]:-rotate-90 class. With
    // showChevron={false} the icon should not render at all inside the
    // single-tool wrapper.
    const wrapper = container.querySelector('[data-role="tool-single"]');
    expect(wrapper).not.toBeNull();
    const trailingChevron = wrapper?.querySelector(
      ".group-data-\\[state\\=closed\\]\\:-rotate-90",
    );
    expect(trailingChevron).toBeNull();
  });

  it("counts the internal-steps disclosure as part of the rail", async () => {
    const user = userEvent.setup();
    const { container } = render(
      <ToolChainCards
        toolItems={[
          pair("Edit · src/a.ts"),
          pair("Edit · src/b.ts"),
          pair("ls"),
          pair("cat"),
        ]}
      />,
    );
    // The chain mounts as already-complete (default test pair → completed),
    // so the rail starts collapsed during replay; expand it first.
    await user.click(
      screen.getByRole("button", { name: /updating files.*4 steps/i }),
    );
    const disclosure = container.querySelector(
      '[data-role="tool-chain-internal-disclosure"]',
    );
    expect(disclosure).not.toBeNull();

    const beforeRows = container.querySelectorAll(
      '[data-role="tool-chain-step"]',
    );
    expect(beforeRows.length).toBeGreaterThanOrEqual(1);

    const showButton = screen.getByRole("button", {
      name: /show internal steps \(2\)/i,
    });
    await user.click(showButton);

    const afterRows = container.querySelectorAll(
      '[data-role="tool-chain-step"]',
    );
    expect(afterRows.length).toBe(beforeRows.length + 2);
  });

  it("removes the heavy parent card chrome around the chain wrapper", () => {
    const { container } = render(
      <ToolChainCards
        toolItems={[pair("Edit · src/a.ts"), pair("Edit · src/b.ts")]}
      />,
    );
    const wrapper = container.querySelector('[data-role="tool-chain-card"]');
    expect(wrapper).not.toBeNull();
    const className = wrapper?.getAttribute("class") ?? "";
    expect(className).not.toMatch(/border-/);
    expect(className).not.toMatch(/bg-muted/);
  });

  it("prefers the LLM chain summary over the deterministic phrase when present", () => {
    const a = pair("Edit · src/a.ts");
    const b = pair("Edit · src/b.ts");
    if (a.request) {
      a.request.chainSummary = {
        summary: "applied dark mode polish",
        count: 2,
      };
    }
    render(<ToolChainCards toolItems={[a, b]} />);
    expect(
      screen.getByRole("button", {
        name: /applied dark mode polish.*2 steps/i,
      }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /updating files/i }),
    ).not.toBeInTheDocument();
  });

  it("falls back to the deterministic phrase when no chain summary is present", () => {
    render(
      <ToolChainCards
        toolItems={[pair("Edit · src/a.ts"), pair("Edit · src/b.ts")]}
      />,
    );
    expect(
      screen.getByRole("button", { name: /updating files.*2 steps/i }),
    ).toBeInTheDocument();
  });

  it("does not surface the chain summary while the chain is still active", () => {
    const a = pair("Edit · src/a.ts");
    const b = pair("Edit · src/b.ts", {
      status: "in_progress",
      completed: false,
    });
    if (a.request) {
      a.request.chainSummary = {
        summary: "applied dark mode polish",
        count: 2,
      };
    }
    render(<ToolChainCards toolItems={[a, b]} />);
    expect(
      screen.getByRole("button", { name: /working through 2 steps/i }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /applied dark mode polish/i }),
    ).not.toBeInTheDocument();
  });
});
