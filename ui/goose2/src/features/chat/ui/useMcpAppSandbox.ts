import type { AppRendererProps, McpUiHostContext } from "@mcp-ui/client";
import { useEffect, useState } from "react";
import type {
  McpAppResourceCsp,
  RenderableMcpAppDocument,
} from "./mcpAppPayload";

type HostColorScheme = NonNullable<McpUiHostContext["theme"]>;
type Sandbox = NonNullable<AppRendererProps["sandbox"]>;

interface GooseServeHostInfo {
  httpBaseUrl: string;
  secretKey: string;
}

function appendDomains(
  params: URLSearchParams,
  key: string,
  domains: string[] | undefined,
) {
  if (domains && domains.length > 0) {
    params.set(key, domains.join(","));
  }
}

function buildProxyUrl(
  httpBaseUrl: string,
  secretKey: string,
  csp: McpAppResourceCsp | null,
  colorScheme: HostColorScheme,
): URL {
  const params = new URLSearchParams({
    secret: secretKey,
    color_scheme: colorScheme,
  });
  appendDomains(params, "connect_domains", csp?.connectDomains);
  appendDomains(params, "resource_domains", csp?.resourceDomains);
  appendDomains(params, "frame_domains", csp?.frameDomains);
  appendDomains(params, "base_uri_domains", csp?.baseUriDomains);
  appendDomains(params, "script_domains", csp?.scriptDomains);
  const proxyBaseUrl = httpBaseUrl.endsWith("/")
    ? httpBaseUrl
    : `${httpBaseUrl}/`;
  return new URL(`mcp-app-proxy?${params.toString()}`, proxyBaseUrl);
}

export function useMcpAppSandbox({
  hostInfo,
  renderableDocument,
  colorScheme,
}: {
  hostInfo: GooseServeHostInfo | null;
  renderableDocument: Pick<RenderableMcpAppDocument, "csp"> | null;
  colorScheme: HostColorScheme;
}): Sandbox | null {
  const [sandboxState, setSandboxState] = useState<{
    signature: string;
    sandbox: Sandbox;
  } | null>(null);

  useEffect(() => {
    if (!hostInfo || !renderableDocument) {
      setSandboxState(null);
      return;
    }

    const signature = JSON.stringify({
      httpBaseUrl: hostInfo.httpBaseUrl,
      secretKey: hostInfo.secretKey,
      csp: renderableDocument.csp,
    });

    setSandboxState((current) => {
      if (current?.signature === signature) {
        return current;
      }

      return {
        signature,
        sandbox: {
          url: buildProxyUrl(
            hostInfo.httpBaseUrl,
            hostInfo.secretKey,
            renderableDocument.csp,
            colorScheme,
          ),
        },
      };
    });
  }, [hostInfo, renderableDocument, colorScheme]);

  return sandboxState?.sandbox ?? null;
}
