import type {
  Implementation,
  InitializeRequest,
} from "@agentclientprotocol/sdk";
import { RESOURCE_MIME_TYPE } from "@modelcontextprotocol/ext-apps";

export const GOOSE_MCP_UI_EXTENSION_ID = "io.modelcontextprotocol/ui" as const;

export interface GooseMcpUiExtensionSettings {
  mimeTypes: string[];
}

export interface GooseMcpHostCapabilities {
  extensions: Record<string, GooseMcpUiExtensionSettings>;
}

export interface GooseClientMeta {
  goose: {
    mcpHostCapabilities: GooseMcpHostCapabilities;
  };
}

export type GooseInitializeRequest = InitializeRequest & {
  clientCapabilities: NonNullable<InitializeRequest["clientCapabilities"]> & {
    _meta: GooseClientMeta;
  };
  clientInfo: Implementation;
};

export const DEFAULT_GOOSE_MCP_HOST_CAPABILITIES: GooseMcpHostCapabilities = {
  extensions: {
    [GOOSE_MCP_UI_EXTENSION_ID]: {
      mimeTypes: [RESOURCE_MIME_TYPE],
    },
  },
};
