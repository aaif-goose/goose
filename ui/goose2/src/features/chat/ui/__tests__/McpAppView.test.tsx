import type { AppRendererProps, RequestHandlerExtra } from "@mcp-ui/client";
import { openUrl } from "@tauri-apps/plugin-opener";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { McpAppView } from "../McpAppView";
import packageJson from "../../../../../package.json";
import type {
  McpAppPayload,
  ToolResponseContent,
} from "@/shared/types/messages";

const mocks = vi.hoisted(() => ({
  appRendererSpy: vi.fn(),
  nestedToolResultSpy: vi.fn(),
  extMethod: vi.fn(),
  getClient: vi.fn(),
  resolvedTheme: "dark" as "light" | "dark",
}));

vi.mock("@mcp-ui/client", () => ({
  UI_EXTENSION_CONFIG: { mimeTypes: ["text/html;profile=mcp-app"] },
  AppRenderer: (props: AppRendererProps) => {
    mocks.appRendererSpy(props);

    return (
      <div>
        <iframe data-testid="mock-app-iframe" title="Mock MCP app" />
        <button
          data-testid="mock-app-renderer"
          onClick={() => {
            void props
              .onCallTool?.(
                {
                  name: "get-server-time",
                  arguments: { timezone: "America/New_York" },
                },
                {} as RequestHandlerExtra,
              )
              .then((result) => {
                mocks.nestedToolResultSpy(result);
              });
          }}
          type="button"
        >
          call nested tool
        </button>
      </div>
    );
  },
}));

vi.mock("@/shared/api/acpConnection", () => ({
  getClient: mocks.getClient,
}));

vi.mock("@/shared/api/gooseServeHost", () => ({
  getGooseServeHostInfo: vi.fn().mockResolvedValue({
    httpBaseUrl: "http://127.0.0.1:4242",
    secretKey: "test-secret",
  }),
}));

vi.mock("@/shared/theme/ThemeProvider", () => ({
  useTheme: () => ({ resolvedTheme: mocks.resolvedTheme }),
}));

function createPayload({
  prefersBorder = true,
}: {
  prefersBorder?: boolean;
} = {}): McpAppPayload {
  return {
    sessionId: "local-session",
    toolCallId: "tool-1",
    toolCallTitle: "inspect messaging",
    source: "toolCallUpdateMeta",
    tool: {
      name: "inspect-messaging",
      extensionName: "mcpappbench_local_",
      resourceUri: "ui://inspect-messaging",
      meta: {
        ui: {
          resourceUri: "ui://inspect-messaging",
        },
        goose_extension: "mcpappbench_local_",
      },
    },
    resource: {
      result: {
        contents: [
          {
            uri: "ui://inspect-messaging",
            mimeType: "text/html;profile=mcp-app",
            text: "<div>Messaging Inspector</div>",
            _meta: {
              ui: {
                prefersBorder,
              },
            },
          },
        ],
      },
    },
  };
}

function createToolResponse(): ToolResponseContent {
  return {
    type: "toolResponse",
    id: "tool-1",
    name: "inspect messaging",
    result: "Messaging Inspector loaded.",
    isError: false,
    structuredContent: {
      timestamp: "2026-04-22T18:28:48.287Z",
      joke: "Why do programmers prefer dark mode? Because light attracts bugs!",
    },
  };
}

function getLatestAppRendererProps(): AppRendererProps {
  const props = mocks.appRendererSpy.mock.calls.at(-1)?.[0] as
    | AppRendererProps
    | undefined;

  expect(props).toBeDefined();
  if (!props) {
    throw new Error("Expected AppRenderer props to be recorded");
  }
  return props;
}

describe("McpAppView nested tool calls", () => {
  beforeEach(() => {
    mocks.appRendererSpy.mockClear();
    mocks.nestedToolResultSpy.mockClear();
    mocks.extMethod.mockReset();
    mocks.getClient.mockReset();
    mocks.resolvedTheme = "dark";
    vi.mocked(openUrl).mockReset();
    vi.mocked(openUrl).mockResolvedValue(undefined);
    mocks.getClient.mockResolvedValue({
      extMethod: mocks.extMethod,
    });
  });

  it("keeps the original toolResult after nested app tool calls resolve", async () => {
    const nestedToolResult = {
      content: [{ type: "text", text: "2026-04-22T18:29:06.433Z" }],
      isError: false,
      structuredContent: {
        timestamp: "2026-04-22T18:29:06.433Z",
        timezone: "America/New_York",
        unixMs: 1776882546433,
      },
      _meta: {
        source: "nested-tool-call",
      },
    };

    mocks.extMethod.mockResolvedValue(nestedToolResult);

    render(
      <McpAppView
        payload={createPayload()}
        toolInput={{ inspector: "messaging" }}
        toolResponse={createToolResponse()}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("mock-app-renderer")).toBeInTheDocument();
    });

    const initialToolResult = getLatestAppRendererProps().toolResult;
    expect(initialToolResult).toEqual(
      expect.objectContaining({
        isError: false,
        structuredContent: expect.objectContaining({
          joke: "Why do programmers prefer dark mode? Because light attracts bugs!",
        }),
      }),
    );

    fireEvent.click(screen.getByTestId("mock-app-renderer"));

    await waitFor(() => {
      expect(mocks.extMethod).toHaveBeenCalledWith("_goose/tool/call", {
        sessionId: "local-session",
        name: "mcpappbench_local___get-server-time",
        arguments: { timezone: "America/New_York" },
      });
    });

    await waitFor(() => {
      expect(mocks.nestedToolResultSpy).toHaveBeenCalledWith(
        expect.objectContaining({
          structuredContent: nestedToolResult.structuredContent,
          _meta: nestedToolResult._meta,
        }),
      );
    });

    const latestProps = getLatestAppRendererProps();
    expect(latestProps.toolInput).toEqual({ timezone: "America/New_York" });
    expect(latestProps.toolResult).toBe(initialToolResult);
    expect(latestProps.toolResult).toEqual(
      expect.objectContaining({
        structuredContent: expect.objectContaining({
          joke: "Why do programmers prefer dark mode? Because light attracts bugs!",
        }),
      }),
    );
  });

  it("only applies rounded border chrome when the app prefers a border", async () => {
    const { rerender } = render(
      <McpAppView
        payload={createPayload()}
        toolResponse={createToolResponse()}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("mock-app-renderer")).toBeInTheDocument();
    });

    const borderedRoot = screen.getByTestId("mcp-app-view");
    expect(borderedRoot.className).not.toContain("md:-mx-4");
    expect(borderedRoot.className).not.toContain("md:w-[calc(100%+2rem)]");

    const borderedChrome = borderedRoot.firstElementChild as HTMLElement | null;
    expect(borderedChrome).not.toBeNull();
    expect(borderedChrome?.className).toContain("rounded-2xl");
    expect(borderedChrome?.className).toContain("border");
    expect(borderedChrome?.className).toContain("[&_iframe]:block");

    rerender(
      <McpAppView
        payload={createPayload({ prefersBorder: false })}
        toolResponse={createToolResponse()}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("mock-app-renderer")).toBeInTheDocument();
    });

    const borderlessRoot = screen.getByTestId("mcp-app-view");
    expect(borderlessRoot.className).not.toContain("md:-mx-4");
    expect(borderlessRoot.className).not.toContain("md:w-[calc(100%+2rem)]");

    const borderlessChrome =
      borderlessRoot.firstElementChild as HTMLElement | null;
    expect(borderlessChrome).not.toBeNull();
    expect(borderlessChrome?.className).not.toContain("rounded-2xl");
    expect(borderlessChrome?.className).not.toContain("border");
    expect(borderlessChrome?.className).not.toContain("shadow-sm");
    expect(borderlessChrome?.className).not.toContain("overflow-hidden");
  });

  it("passes Goose2 package identity in host context", async () => {
    render(
      <McpAppView
        payload={createPayload()}
        toolResponse={createToolResponse()}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("mock-app-renderer")).toBeInTheDocument();
    });

    expect(getLatestAppRendererProps().hostContext?.userAgent).toBe(
      `${packageJson.name}/${packageJson.version}`,
    );
  });

  it("does not install a fallback handler for non-standard app requests", async () => {
    render(
      <McpAppView
        payload={createPayload()}
        toolResponse={createToolResponse()}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("mock-app-renderer")).toBeInTheDocument();
    });

    expect(getLatestAppRendererProps().onFallbackRequest).toBeUndefined();
  });

  it("confirms safe app open-link requests before opening the URL", async () => {
    render(
      <McpAppView
        payload={createPayload()}
        toolResponse={createToolResponse()}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("mock-app-renderer")).toBeInTheDocument();
    });

    const promise = getLatestAppRendererProps().onOpenLink?.(
      { url: "https://example.com" },
      {} as RequestHandlerExtra,
    );

    if (!promise) {
      throw new Error("Expected onOpenLink to be registered");
    }

    expect(openUrl).not.toHaveBeenCalled();

    await waitFor(() => {
      expect(screen.getByText("https://example.com/")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole("button", { name: "Open link" }));

    await waitFor(() => {
      expect(openUrl).toHaveBeenCalledWith("https://example.com/");
    });
    await expect(promise).resolves.toEqual({});
  });

  it("blocks app open-link requests for non-web URL schemes", async () => {
    render(
      <McpAppView
        payload={createPayload()}
        toolResponse={createToolResponse()}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("mock-app-renderer")).toBeInTheDocument();
    });

    const result = await getLatestAppRendererProps().onOpenLink?.(
      { url: "file:///private/tmp/secrets.txt" },
      {} as RequestHandlerExtra,
    );

    expect(result).toEqual(
      expect.objectContaining({
        isError: true,
      }),
    );
    expect(openUrl).not.toHaveBeenCalled();
    expect(
      screen.queryByText("file:///private/tmp/secrets.txt"),
    ).not.toBeInTheDocument();
  });

  it("keeps the iframe color scheme aligned with the host theme", async () => {
    mocks.resolvedTheme = "light";
    const payload = createPayload();
    const toolResponse = createToolResponse();

    const { rerender } = render(
      <McpAppView payload={payload} toolResponse={toolResponse} />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("mock-app-renderer")).toBeInTheDocument();
    });

    const appChrome = screen.getByTestId("mcp-app-view")
      .firstElementChild as HTMLElement | null;
    expect(appChrome).not.toBeNull();
    expect(appChrome?.style.colorScheme).toBe("light");

    const iframe = screen.getByTestId("mock-app-iframe") as HTMLIFrameElement;
    await waitFor(() => {
      expect(iframe.style.getPropertyValue("color-scheme")).toBe("light");
      expect(iframe.style.backgroundColor).toBe("transparent");
    });
    expect(
      getLatestAppRendererProps().sandbox?.url.searchParams.get("color_scheme"),
    ).toBe("light");
    expect(getLatestAppRendererProps().hostContext?.theme).toBe("light");

    const initialSandbox = getLatestAppRendererProps().sandbox;

    mocks.resolvedTheme = "dark";
    rerender(<McpAppView payload={payload} toolResponse={toolResponse} />);

    await waitFor(() => {
      expect(appChrome?.style.colorScheme).toBe("dark");
    });
    expect(getLatestAppRendererProps().hostContext?.theme).toBe("dark");
    expect(getLatestAppRendererProps().sandbox).toBe(initialSandbox);
    expect(
      getLatestAppRendererProps().sandbox?.url.searchParams.get("color_scheme"),
    ).toBe("light");
  });

  it("declares readily available host context fields", async () => {
    const rectSpy = vi
      .spyOn(HTMLElement.prototype, "getBoundingClientRect")
      .mockReturnValue({
        x: 0,
        y: 0,
        width: 736,
        height: 240,
        top: 0,
        right: 736,
        bottom: 240,
        left: 0,
        toJSON: () => ({}),
      } as DOMRect);
    const matchMediaSpy = vi.fn((query: string) => ({
      matches: query === "(hover: hover)",
      media: query,
      onchange: null,
      addListener: () => {},
      removeListener: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    }));
    const originalMatchMedia = window.matchMedia;
    window.matchMedia = matchMediaSpy;

    try {
      render(
        <McpAppView
          payload={createPayload()}
          toolResponse={createToolResponse()}
        />,
      );

      await waitFor(() => {
        expect(screen.getByTestId("mock-app-renderer")).toBeInTheDocument();
      });

      expect(getLatestAppRendererProps().hostContext).toEqual(
        expect.objectContaining({
          theme: "dark",
          displayMode: "inline",
          availableDisplayModes: ["inline"],
          containerDimensions: {
            width: 736,
            height: 240,
          },
          locale: navigator.language,
          timeZone: Intl.DateTimeFormat().resolvedOptions().timeZone,
          userAgent: expect.stringMatching(/^goose2\//),
          platform: "desktop",
          deviceCapabilities: {
            touch: false,
            hover: true,
          },
          safeAreaInsets: {
            top: 0,
            right: 0,
            bottom: 0,
            left: 0,
          },
          toolInfo: {
            id: "tool-1",
            tool: {
              name: "inspect-messaging",
              title: "inspect messaging",
              inputSchema: {
                type: "object",
              },
              _meta: {
                ui: {
                  resourceUri: "ui://inspect-messaging",
                },
                goose_extension: "mcpappbench_local_",
              },
            },
          },
        }),
      );
    } finally {
      window.matchMedia = originalMatchMedia;
      rectSpy.mockRestore();
    }
  });
});
