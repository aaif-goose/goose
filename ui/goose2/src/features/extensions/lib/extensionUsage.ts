import type { Message, ToolRequestContent } from "@/shared/types/messages";
import type { SessionExtensionStatus } from "../types";
import { normalizeExtensionKey } from "./extensionKeys";

export interface ExtensionUsage {
  count: number;
  lastUsedAt: number;
}

function formatFallbackDisplayName(configKey: string): string {
  const words = configKey.split(/[_-]+/).filter(Boolean);
  if (words.length === 0) return configKey;
  return words
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(" ");
}

function unavailableExtension(configKey: string): SessionExtensionStatus {
  return {
    type: "platform",
    name: configKey,
    description: "",
    display_name: formatFallbackDisplayName(configKey),
    config_key: configKey,
    status: "unavailable",
    tools: [],
  };
}

function toolOwnerFromName(name: string): string | null {
  const [owner] = name.split("__");
  return owner && owner !== name ? normalizeExtensionKey(owner) : null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function persistedExtensionName(
  toolRequest: ToolRequestContent,
): string | null {
  const meta = (toolRequest as unknown as { _meta?: unknown })._meta;
  if (!isRecord(meta)) return null;
  const extensionName = meta.goose_extension;
  return typeof extensionName === "string" ? extensionName : null;
}

function persistedToolName(toolRequest: ToolRequestContent): string | null {
  const toolCall = (toolRequest as unknown as { toolCall?: unknown }).toolCall;
  if (!isRecord(toolCall)) return null;

  const value = toolCall.value;
  if (isRecord(value) && typeof value.name === "string") {
    return value.name;
  }
  if (typeof toolCall.name === "string") {
    return toolCall.name;
  }
  return null;
}

function getToolOwnerFromName(
  toolName: string,
  toolToExtension: Map<string, string>,
): string | null {
  return (
    toolToExtension.get(normalizeExtensionKey(toolName)) ??
    toolOwnerFromName(toolName)
  );
}

export function buildToolToExtensionMap(
  extensions: SessionExtensionStatus[],
): Map<string, string> {
  const byTool = new Map<string, string>();
  for (const extension of extensions) {
    const configKey = normalizeExtensionKey(extension.config_key);
    const nameKey = normalizeExtensionKey(extension.name);
    if (!byTool.has(configKey)) {
      byTool.set(configKey, extension.config_key);
    }
    if (!byTool.has(nameKey)) {
      byTool.set(nameKey, extension.config_key);
    }

    for (const tool of extension.tools) {
      byTool.set(normalizeExtensionKey(tool), extension.config_key);
      const unprefixedName = tool.split("__").pop();
      const unprefixedKey = unprefixedName
        ? normalizeExtensionKey(unprefixedName)
        : null;
      if (unprefixedKey && !byTool.has(unprefixedKey)) {
        byTool.set(unprefixedKey, extension.config_key);
      }
    }
  }
  return byTool;
}

export function getToolOwner(
  toolRequest: ToolRequestContent,
  toolToExtension: Map<string, string>,
): string | null {
  const extensionName =
    toolRequest.extensionName ?? persistedExtensionName(toolRequest);
  if (extensionName) {
    return (
      toolToExtension.get(normalizeExtensionKey(extensionName)) ??
      normalizeExtensionKey(extensionName)
    );
  }
  const toolName = toolRequest.toolName ?? persistedToolName(toolRequest);
  if (toolName) {
    return getToolOwnerFromName(toolName, toolToExtension);
  }
  if (!toolRequest.name) {
    return null;
  }
  return getToolOwnerFromName(toolRequest.name, toolToExtension);
}

export function getToolOwnerSignatureKey(
  toolRequest: ToolRequestContent,
): string {
  const extensionName =
    toolRequest.extensionName ?? persistedExtensionName(toolRequest);
  if (extensionName) {
    return normalizeExtensionKey(extensionName);
  }

  const toolName =
    toolRequest.toolName ?? persistedToolName(toolRequest) ?? toolRequest.name;
  if (!toolName) {
    return "";
  }
  return toolOwnerFromName(toolName) ?? normalizeExtensionKey(toolName);
}

export function getExtensionUsageByConfigKey(
  messages: Message[],
  toolToExtension: Map<string, string>,
): Map<string, ExtensionUsage> {
  const usage = new Map<string, ExtensionUsage>();
  for (const message of messages) {
    for (const content of message.content) {
      if (content.type !== "toolRequest") continue;
      const owner = getToolOwner(content, toolToExtension);
      if (!owner) continue;
      const previous = usage.get(owner);
      usage.set(owner, {
        count: (previous?.count ?? 0) + 1,
        lastUsedAt: Math.max(previous?.lastUsedAt ?? 0, message.created),
      });
    }
  }
  return usage;
}

export function getUsedSessionExtensions(
  extensions: SessionExtensionStatus[],
  messages: Message[],
): SessionExtensionStatus[] {
  const toolToExtension = buildToolToExtensionMap(extensions);
  const usageByExtension = getExtensionUsageByConfigKey(
    messages,
    toolToExtension,
  );
  const extensionsByKey = new Map(
    extensions.map((extension) => [extension.config_key, extension]),
  );

  return Array.from(usageByExtension.keys())
    .map(
      (configKey) =>
        extensionsByKey.get(configKey) ?? unavailableExtension(configKey),
    )
    .sort((a, b) => {
      const aUsage = usageByExtension.get(a.config_key)?.lastUsedAt ?? 0;
      const bUsage = usageByExtension.get(b.config_key)?.lastUsedAt ?? 0;
      return bUsage - aUsage;
    });
}
