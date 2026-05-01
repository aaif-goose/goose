import { describe, expect, it } from "vitest";
import type { Message } from "@/shared/types/messages";
import type { SessionExtensionStatus } from "../../types";
import {
  buildToolToExtensionMap,
  getExtensionUsageByConfigKey,
  getUsedSessionExtensions,
} from "../extensionUsage";

function extension(configKey: string, tools: string[]): SessionExtensionStatus {
  return {
    type: "builtin",
    name: configKey,
    description: `${configKey} extension`,
    config_key: configKey,
    status: "connected",
    tools,
  };
}

function toolRequestMessage(
  created: number,
  request: {
    name: string;
    toolName?: string;
    extensionName?: string;
  },
): Message {
  return {
    id: `message-${created}`,
    role: "assistant",
    created,
    content: [
      {
        type: "toolRequest",
        id: `tool-${created}`,
        arguments: {},
        status: "completed",
        ...request,
      },
    ],
  };
}

describe("extension usage derivation", () => {
  it("uses explicit extension metadata when present", () => {
    const extensions = [extension("github", ["github__create_issue"])];
    const toolMap = buildToolToExtensionMap(extensions);
    const usage = getExtensionUsageByConfigKey(
      [
        toolRequestMessage(10, {
          name: "Create issue",
          extensionName: "Git Hub",
        }),
      ],
      toolMap,
    );

    expect(usage.get("github")).toEqual({ count: 1, lastUsedAt: 10 });
  });

  it("maps unprefixed tool names back to their extension", () => {
    const extensions = [extension("weather", ["weather__forecast"])];
    const used = getUsedSessionExtensions(extensions, [
      toolRequestMessage(20, {
        name: "Forecast",
        toolName: "forecast",
      }),
    ]);

    expect(used.map((item) => item.config_key)).toEqual(["weather"]);
  });

  it("falls back to prefixed display names when status tools are unavailable", () => {
    const extensions = [extension("jira", [])];
    const used = getUsedSessionExtensions(extensions, [
      toolRequestMessage(30, {
        name: "jira__create_ticket",
      }),
    ]);

    expect(used.map((item) => item.config_key)).toEqual(["jira"]);
  });

  it("sorts used extensions by latest tool request", () => {
    const extensions = [
      extension("older", ["older__read"]),
      extension("newer", ["newer__read"]),
    ];
    const used = getUsedSessionExtensions(extensions, [
      toolRequestMessage(10, { name: "older__read" }),
      toolRequestMessage(40, { name: "newer__read" }),
      toolRequestMessage(20, { name: "older__read" }),
    ]);

    expect(used.map((item) => item.config_key)).toEqual(["newer", "older"]);
  });

  it("keeps historical usage when the extension is no longer in session status", () => {
    const used = getUsedSessionExtensions(
      [],
      [
        toolRequestMessage(50, {
          name: "Create issue",
          extensionName: "Git Hub",
        }),
      ],
    );

    expect(used).toEqual([
      expect.objectContaining({
        config_key: "github",
        display_name: "Github",
        status: "unavailable",
      }),
    ]);
  });

  it("maps an unprefixed tool call to a configured extension with the same key", () => {
    const extensions = [extension("analyze", [])];
    const used = getUsedSessionExtensions(extensions, [
      toolRequestMessage(60, {
        name: "analyze",
        toolName: "analyze",
      }),
    ]);

    expect(used.map((item) => item.config_key)).toEqual(["analyze"]);
  });
});
