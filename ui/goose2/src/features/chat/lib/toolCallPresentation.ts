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

export interface ToolInputSummaryRow {
  label: string;
  value: string;
  monospace?: boolean;
  /** Full path/value for hover tooltip when `value` was shortened. */
  title?: string;
  /** Hint for syntax-highlighting downstream renderers. */
  renderAs?: "text" | "bash";
}

interface ToolCallPresentationInput {
  name: string;
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

function basenameOf(path: string): string {
  const normalized = path.replace(/\\/g, "/");
  const parts = normalized.split("/").filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

/**
 * Translate raw tool arguments into a small set of labeled rows for the
 * expanded tool card. Falls back to an empty list when no familiar shape is
 * found, leaving the JSON dump as the canonical representation.
 *
 * Slim port of `toolCallPresentation.ts` from PR #8773 — that version also
 * leaned on `kind` / `locations` on the wire. The current main does not carry
 * those fields, so this version is args-only.
 */
export function getToolInputSummaryRows({
  name,
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
  if (query) {
    const path = getStringArgument(args, PATH_KEYS);
    return [
      { label: "Query", value: query, monospace: true },
      ...(path ? [{ label: "Path", value: path, monospace: true }] : []),
    ];
  }

  const url = getStringArgument(args, URL_KEYS);
  if (url) {
    return [{ label: "Resource", value: url, monospace: true }];
  }

  const path = getStringArgument(args, PATH_KEYS);
  if (path) {
    const line = getNumericArgument(args, ["line", "startLine"]);
    const displayPath = basenameOf(path);
    return [
      {
        label: "Path",
        value: displayPath,
        monospace: true,
        title: path,
      },
      ...(line ? [{ label: "Line", value: String(line) }] : []),
    ];
  }

  if (name.trim().length > 0) {
    return [{ label: "Tool", value: name }];
  }

  return [];
}
