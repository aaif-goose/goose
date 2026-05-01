import { describe, expect, it } from "vitest";
import type { ExtensionEntry, SessionExtensionStatus } from "../../types";
import {
  getToolUsageSnapshot,
  mergeExtensionStatuses,
  toUnavailableSessionExtensionStatus,
  type ToolUsageSnapshot,
} from "../extensionsWidgetData";
import type { Message } from "@/shared/types/messages";

function toolRequestMessage(extensionName: string, id = "message-1"): Message {
  return {
    id,
    role: "assistant",
    created: 10,
    content: [
      {
        type: "toolRequest",
        id: "tool-1",
        name: "analyze",
        toolName: "analyze",
        extensionName,
        arguments: {},
        status: "completed",
      },
      { type: "text", text: "done" },
    ],
  };
}

const configuredExtension: ExtensionEntry = {
  type: "stdio",
  name: "Analyze",
  description: "Analyze things",
  cmd: "analyze",
  args: [],
  config_key: "analyze",
  enabled: true,
};

const connectedExtension: SessionExtensionStatus = {
  type: "stdio",
  name: "Analyze",
  description: "Analyze things",
  cmd: "analyze",
  args: [],
  config_key: "analyze",
  status: "connected",
  tools: ["analyze__run"],
};

describe("extensions widget data helpers", () => {
  it("keeps only tool requests in the usage snapshot", () => {
    const previous: ToolUsageSnapshot = { signature: "", messages: [] };
    const snapshot = getToolUsageSnapshot(
      [toolRequestMessage("analyze")],
      previous,
    );

    expect(snapshot.signature).toBe("message-1:tool-1:analyze");
    expect(snapshot.messages).toHaveLength(1);
    expect(snapshot.messages[0].content).toEqual([
      expect.objectContaining({ type: "toolRequest", id: "tool-1" }),
    ]);
  });

  it("reuses the previous snapshot when the tool signature is unchanged", () => {
    const previous = getToolUsageSnapshot([toolRequestMessage("analyze")], {
      signature: "",
      messages: [],
    });

    expect(
      getToolUsageSnapshot([toolRequestMessage("analyze")], previous),
    ).toBe(previous);
  });

  it("adds configured extensions as unavailable when status omits them", () => {
    expect(mergeExtensionStatuses([], [configuredExtension])).toEqual([
      {
        type: "stdio",
        name: "Analyze",
        description: "Analyze things",
        cmd: "analyze",
        args: [],
        config_key: "analyze",
        status: "unavailable",
        tools: [],
      },
    ]);
  });

  it("keeps connected session status when configured data has the same key", () => {
    expect(
      mergeExtensionStatuses([connectedExtension], [configuredExtension]),
    ).toEqual([connectedExtension]);
  });

  it("drops the config enabled flag when building unavailable status", () => {
    expect(
      toUnavailableSessionExtensionStatus(configuredExtension),
    ).not.toHaveProperty("enabled");
  });
});
