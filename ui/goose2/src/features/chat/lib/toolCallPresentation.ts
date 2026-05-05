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

/**
 * Stable, locale-independent identifier for a row's label. The renderer
 * resolves this to a translated string at draw time via
 * `chat.tools.inputSummary.<kind>`. Keeping the identifier on the row
 * (rather than baking the English label in here) lets downstream code
 * branch on row identity (e.g. headers that pull the path row) without
 * snapping to a particular locale.
 */
export type ToolInputSummaryRowKind =
  | "command"
  | "workingDirectory"
  | "query"
  | "path"
  | "resource"
  | "line"
  | "tool";

export interface ToolInputSummaryRow {
  kind: ToolInputSummaryRowKind;
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
        kind: "command",
        value: command,
        monospace: true,
        renderAs: "bash",
      },
      ...(cwd
        ? [
            {
              kind: "workingDirectory" as const,
              value: cwd,
              monospace: true,
            },
          ]
        : []),
    ];
  }

  const query = getStringArgument(args, SEARCH_KEYS);
  if (query) {
    const path = getStringArgument(args, PATH_KEYS);
    return [
      { kind: "query", value: query, monospace: true },
      ...(path
        ? [{ kind: "path" as const, value: path, monospace: true }]
        : []),
    ];
  }

  const url = getStringArgument(args, URL_KEYS);
  if (url) {
    return [{ kind: "resource", value: url, monospace: true }];
  }

  const path = getStringArgument(args, PATH_KEYS);
  if (path) {
    const line = getNumericArgument(args, ["line", "startLine"]);
    const displayPath = basenameOf(path);
    return [
      {
        kind: "path",
        value: displayPath,
        monospace: true,
        title: path,
      },
      ...(line ? [{ kind: "line" as const, value: String(line) }] : []),
    ];
  }

  if (name.trim().length > 0) {
    return [{ kind: "tool", value: name }];
  }

  return [];
}

/**
 * Maximum length (in characters, after trim) for a text result to be eligible
 * for hoisting into the tool header subtitle. Longer values stay in the body.
 */
const HOISTABLE_TEXT_MAX_LENGTH = 80;

/**
 * Returns true when a tool's text `result` is short enough and simple enough
 * to render as a header subtitle alongside the tool name. Multi-line and
 * empty/whitespace-only strings are never hoistable.
 */
export function isHoistableText(text: string | undefined): text is string {
  if (typeof text !== "string") return false;
  const trimmed = text.trim();
  if (trimmed.length === 0) return false;
  if (trimmed.includes("\n") || trimmed.includes("\r")) return false;
  return trimmed.length <= HOISTABLE_TEXT_MAX_LENGTH;
}

/**
 * Returns true when a text `result` is just a stringified copy of the
 * `structured` content — e.g. a server that emits `result = JSON.stringify(x)`
 * alongside `structuredContent = x`. Whitespace- and indent-insensitive: the
 * comparison normalizes both sides via JSON parse + compact stringify.
 *
 * Used by the de-dupe matrix in ToolCallAdapter to suppress the redundant
 * text result when the structured form will already be rendered.
 */
export function isStringifiedCopyOfStructured(
  text: string | undefined,
  structured: unknown,
): boolean {
  if (typeof text !== "string" || structured === undefined) return false;

  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return false;
  }

  try {
    return JSON.stringify(parsed) === JSON.stringify(structured);
  } catch {
    return false;
  }
}
