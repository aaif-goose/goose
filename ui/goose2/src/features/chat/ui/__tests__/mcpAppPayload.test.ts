import { describe, expect, it } from "vitest";
import { extractRenderableMcpAppDocument } from "../mcpAppPayload";
import type { McpAppPayload } from "@/shared/types/messages";

function createPayload(csp: unknown): McpAppPayload {
  return {
    sessionId: "session-1",
    toolCallId: "tool-1",
    toolCallTitle: "inspect app",
    source: "toolCallUpdateMeta",
    tool: {
      name: "inspect-app",
      extensionName: "mcpappbench",
      resourceUri: "ui://inspect-app",
    },
    resource: {
      result: {
        contents: [
          {
            uri: "ui://inspect-app",
            mimeType: "text/html;profile=mcp-app",
            text: "<div>App</div>",
            _meta: {
              ui: {
                csp,
              },
            },
          },
        ],
      },
    },
  };
}

describe("extractRenderableMcpAppDocument", () => {
  it("normalizes MCP app CSP metadata to string arrays", () => {
    const document = extractRenderableMcpAppDocument(
      createPayload({
        connectDomains: "https://api.example.com",
        resourceDomains: ["https://cdn.example.com", 42],
        frameDomains: ["https://frame.example.com"],
        baseUriDomains: { origin: "https://base.example.com" },
        scriptDomains: ["https://scripts.example.com"],
      }),
    );

    expect(document?.csp).toEqual({
      resourceDomains: ["https://cdn.example.com"],
      frameDomains: ["https://frame.example.com"],
      scriptDomains: ["https://scripts.example.com"],
    });
  });

  it("drops malformed MCP app CSP metadata", () => {
    const document = extractRenderableMcpAppDocument(
      createPayload({
        connectDomains: "https://api.example.com",
        resourceDomains: [],
        frameDomains: { origin: "https://frame.example.com" },
      }),
    );

    expect(document?.csp).toBeNull();
  });
});
