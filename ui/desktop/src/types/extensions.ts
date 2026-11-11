import type { RecipeExtensionDto } from '@aaif/goose-acp-client';

export type Envs = Record<string, string>;

export type ToolCallMeta = Record<string, unknown>;

// The published DTO has no tool_call_meta. ACP carries it in the MCP server's
// `_meta.toolCallMeta`, and it is kept here so a save from the desktop doesn't drop it.
export type ExtensionConfig = RecipeExtensionDto & { tool_call_meta?: ToolCallMeta };

export type ExtensionEntry = ExtensionConfig & {
  enabled: boolean;
};

export type ExtensionLoadResult = {
  error?: string | null;
  name: string;
  success: boolean;
};
