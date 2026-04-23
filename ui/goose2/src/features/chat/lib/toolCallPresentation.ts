import type { ToolCallKind, ToolCallLocation } from "@/shared/types/messages";

const COMMAND_KEYS = ["command", "cmd", "script"];
const SEARCH_KEYS = ["query", "pattern", "search", "needle", "text"];
const PATH_KEYS = [
  "path",
  "file",
  "filePath",
  "filepath",
  "targetPath",
  "directory",
  "dir",
  "cwd",
  "folder",
];
const URL_KEYS = ["url", "uri", "href"];
const FILE_ORIENTED_KINDS = new Set<ToolCallKind>([
  "read",
  "edit",
  "delete",
  "move",
]);

export interface ToolInputSummaryRow {
  label: string;
  value: string;
  monospace?: boolean;
  title?: string;
  renderAs?: "text" | "bash";
}

interface ToolCallPresentationInput {
  name: string;
  kind?: ToolCallKind;
  locations?: ToolCallLocation[];
  arguments: Record<string, unknown>;
}

function getStringArgument(
  args: Record<string, unknown>,
  keys: string[],
): string | undefined {
  for (const key of keys) {
    const value = args[key];
    if (typeof value === "string" && value.trim().length > 0) {
      return value.trim();
    }
  }

  return undefined;
}

function getNumericArgument(
  args: Record<string, unknown>,
  keys: string[],
): number | undefined {
  for (const key of keys) {
    const value = args[key];
    if (typeof value === "number" && Number.isFinite(value)) {
      return value;
    }
  }

  return undefined;
}

function looksLikeUrl(value: string): boolean {
  return /^https?:\/\//i.test(value);
}

function basenameOf(path: string): string {
  const normalized = path.replace(/\\/g, "/");
  const parts = normalized.split("/").filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

function getPrimaryPath(
  args: Record<string, unknown>,
  locations?: ToolCallLocation[],
): string | undefined {
  return getStringArgument(args, PATH_KEYS) ?? locations?.[0]?.path;
}

export function getToolInputSummaryRows({
  name,
  kind,
  locations,
  arguments: args,
}: ToolCallPresentationInput): ToolInputSummaryRow[] {
  const command = getStringArgument(args, COMMAND_KEYS);
  if (command) {
    const cwd = getStringArgument(args, ["cwd"]);
    return [
      {
        label: "Command",
        value: command,
        monospace: true,
        renderAs: "bash",
      },
      ...(cwd
        ? [{ label: "Working directory", value: cwd, monospace: true }]
        : []),
    ];
  }

  const query = getStringArgument(args, SEARCH_KEYS);
  if (kind === "search" || query) {
    const path = getPrimaryPath(args, locations);
    return [
      ...(query ? [{ label: "Query", value: query, monospace: true }] : []),
      ...(path ? [{ label: "Path", value: path, monospace: true }] : []),
    ];
  }

  const url = getStringArgument(args, URL_KEYS);
  if (kind === "fetch" || url) {
    return url ? [{ label: "Resource", value: url, monospace: true }] : [];
  }

  const path = getPrimaryPath(args, locations);
  if ((kind && FILE_ORIENTED_KINDS.has(kind)) || path) {
    const line =
      getNumericArgument(args, ["line", "startLine"]) ?? locations?.[0]?.line;
    const displayPath = path ? basenameOf(path) : undefined;
    return [
      ...(path
        ? [
            {
              label: "Path",
              value: displayPath ?? path,
              monospace: true,
              title: path,
            },
          ]
        : []),
      ...(line ? [{ label: "Line", value: String(line) }] : []),
    ];
  }

  if (name.trim().length > 0) {
    return [{ label: "Tool", value: name }];
  }

  return [];
}

export function isFileOrientedToolCall({
  kind,
  locations,
  arguments: args,
}: Omit<ToolCallPresentationInput, "name">): boolean {
  if (locations && locations.length > 0) {
    return true;
  }

  if (kind && FILE_ORIENTED_KINDS.has(kind)) {
    return true;
  }

  const path = getStringArgument(args, PATH_KEYS);
  return Boolean(path && !looksLikeUrl(path));
}

export function dedupeToolLocations(
  locations?: ToolCallLocation[],
): ToolCallLocation[] {
  if (!locations?.length) {
    return [];
  }

  const deduped = new Map<string, ToolCallLocation>();
  for (const location of locations) {
    const key = `${location.path}:${location.line ?? ""}`;
    if (!deduped.has(key)) {
      deduped.set(key, location);
    }
  }

  return Array.from(deduped.values());
}

export function getToolLocationTitle(location: ToolCallLocation): string {
  return location.line
    ? `${basenameOf(location.path)}:${location.line}`
    : basenameOf(location.path);
}

export function getToolLocationSubtitle(location: ToolCallLocation): string {
  return location.line ? `${location.path}:${location.line}` : location.path;
}
