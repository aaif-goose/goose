import type {
  GooseResourceMetadata,
  GooseTextResourceContents,
} from "@aaif/goose-sdk";
import { UI_EXTENSION_CONFIG } from "@mcp-ui/client";
import type { McpAppPayload } from "@/shared/types/messages";

export interface McpAppResourceCsp {
  connectDomains?: string[];
  resourceDomains?: string[];
  frameDomains?: string[];
  baseUriDomains?: string[];
  scriptDomains?: string[];
}

export interface RenderableMcpAppDocument {
  html: string;
  resourceUri: string;
  csp: McpAppResourceCsp | null;
  prefersBorder: boolean;
}

type TextContentWithMeta = GooseTextResourceContents & {
  _meta?: Record<string, unknown>;
  meta?: Record<string, unknown>;
};

const MCP_APP_RESOURCE_MIME_TYPE = UI_EXTENSION_CONFIG.mimeTypes[0];
const MCP_APP_CSP_FIELDS = [
  "connectDomains",
  "resourceDomains",
  "frameDomains",
  "baseUriDomains",
  "scriptDomains",
] satisfies readonly (keyof McpAppResourceCsp)[];

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function isTextContent(content: unknown): content is TextContentWithMeta {
  return isRecord(content) && typeof content.text === "string";
}

function getResourceMetadata(
  content: TextContentWithMeta,
): GooseResourceMetadata | null {
  const meta = isRecord(content._meta)
    ? content._meta
    : isRecord(content.meta)
      ? content.meta
      : null;
  const ui = meta?.ui;
  return isRecord(ui) ? (ui as GooseResourceMetadata) : null;
}

function getContentPriority(content: TextContentWithMeta): number {
  if (content.mimeType === MCP_APP_RESOURCE_MIME_TYPE) {
    return 0;
  }

  if (content.mimeType?.startsWith("text/html")) {
    return 1;
  }

  return 2;
}

function normalizeCspDomains(value: unknown): string[] | undefined {
  if (!Array.isArray(value)) {
    return undefined;
  }

  const domains = value.filter(
    (domain): domain is string => typeof domain === "string",
  );
  return domains.length > 0 ? domains : undefined;
}

function normalizeResourceCsp(csp: unknown): McpAppResourceCsp | null {
  if (!isRecord(csp)) {
    return null;
  }

  const normalized: McpAppResourceCsp = {};
  for (const field of MCP_APP_CSP_FIELDS) {
    const domains = normalizeCspDomains(csp[field]);
    if (domains) {
      normalized[field] = domains;
    }
  }

  return Object.keys(normalized).length > 0 ? normalized : null;
}

export function extractRenderableMcpAppDocument(
  payload: McpAppPayload,
): RenderableMcpAppDocument | null {
  const textContents = (payload.resource.result?.contents ?? []).filter(
    isTextContent,
  );

  if (textContents.length === 0) {
    return null;
  }

  const bestContent = [...textContents].sort(
    (left, right) => getContentPriority(left) - getContentPriority(right),
  )[0];

  if (!bestContent.text) {
    return null;
  }

  const metadata = getResourceMetadata(bestContent);
  const csp = metadata?.csp;

  return {
    html: bestContent.text,
    resourceUri: bestContent.uri ?? payload.tool.resourceUri,
    csp: normalizeResourceCsp(csp),
    prefersBorder: metadata?.prefersBorder ?? true,
  };
}
