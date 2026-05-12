import { describe, expect, it } from "vitest";
import {
  getChainAggregateStatus,
  getToolItemName,
  getToolItemStatus,
  shouldRenderAsGroupedChain,
  type ToolChainItem,
} from "../toolChainGrouping";
import type {
  ToolRequestContent,
  ToolResponseContent,
} from "@/shared/types/messages";

function makeRequest(
  overrides: Partial<ToolRequestContent> = {},
): ToolRequestContent {
  return {
    type: "toolRequest",
    id: "req-1",
    name: "tool name",
    arguments: {},
    status: "completed",
    ...overrides,
  };
}

function makeResponse(
  overrides: Partial<ToolResponseContent> = {},
): ToolResponseContent {
  return {
    type: "toolResponse",
    id: "req-1",
    name: "tool name",
    result: "ok",
    isError: false,
    ...overrides,
  };
}

function pair(
  key: string,
  request?: Partial<ToolRequestContent>,
  response?: Partial<ToolResponseContent>,
): ToolChainItem {
  return {
    key,
    request: request ? makeRequest(request) : undefined,
    response: response ? makeResponse(response) : undefined,
  };
}

describe("getToolItemName", () => {
  it("uses request name when present", () => {
    expect(getToolItemName(pair("a", { name: "edit foo" }))).toBe("edit foo");
  });

  it("falls back to response name", () => {
    expect(getToolItemName(pair("a", undefined, { name: "ran sh" }))).toBe(
      "ran sh",
    );
  });

  it("falls back to a generic label when neither has a name", () => {
    expect(getToolItemName(pair("a", { name: "" }, { name: "" }))).toBe(
      "Tool result",
    );
  });
});

describe("getToolItemStatus", () => {
  it("treats response presence as completed", () => {
    expect(getToolItemStatus(pair("a", { name: "x" }, {}))).toBe("completed");
  });

  it("treats response.isError as error", () => {
    expect(getToolItemStatus(pair("a", { name: "x" }, { isError: true }))).toBe(
      "failed",
    );
  });

  it("uses request status when no response yet", () => {
    expect(getToolItemStatus(pair("a", { status: "in_progress" }))).toBe(
      "in_progress",
    );
  });
});

describe("getChainAggregateStatus", () => {
  it("prefers error over pending and executing", () => {
    expect(
      getChainAggregateStatus([
        pair("a", { status: "in_progress" }),
        pair("b", { name: "x" }, { isError: true }),
        pair("c", { status: "pending" }),
      ]),
    ).toBe("failed");
  });

  it("prefers stopped over executing/pending when there is no error", () => {
    expect(
      getChainAggregateStatus([
        pair("a", { status: "in_progress" }),
        pair("b", { status: "stopped" }),
      ]),
    ).toBe("stopped");
  });

  it("returns executing when any request is executing and none failed", () => {
    expect(
      getChainAggregateStatus([
        pair("a", { name: "x" }, {}),
        pair("b", { status: "in_progress" }),
      ]),
    ).toBe("in_progress");
  });

  it("returns pending when only pending is present", () => {
    expect(
      getChainAggregateStatus([
        pair("a", { status: "pending" }),
        pair("b", { status: "pending" }),
      ]),
    ).toBe("pending");
  });

  it("returns completed when every step finished cleanly", () => {
    expect(
      getChainAggregateStatus([
        pair("a", { name: "x" }, {}),
        pair("b", { name: "y" }, {}),
      ]),
    ).toBe("completed");
  });
});

describe("shouldRenderAsGroupedChain", () => {
  it("is false for single-item sections", () => {
    expect(shouldRenderAsGroupedChain([pair("a", { name: "x" })])).toBe(false);
  });

  it("is true once there are 2+ items", () => {
    expect(
      shouldRenderAsGroupedChain([
        pair("a", { name: "x" }),
        pair("b", { name: "y" }),
      ]),
    ).toBe(true);
  });
});
