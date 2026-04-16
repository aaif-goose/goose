import {
  collectCommandArgPathCandidates,
  extractToolNamePathCandidates,
} from "@/features/chat/lib/artifactPathCommandExtraction";

export type ArtifactCandidateSource =
  | "arg_key"
  | "result_regex"
  | "markdown_href";
export type ArtifactCandidateConfidence = "high" | "low";
export type ArtifactCandidateKind = "file" | "folder" | "path";

export interface ArtifactPathCandidate {
  id: string;
  rawPath: string;
  resolvedPath: string;
  source: ArtifactCandidateSource;
  confidence: ArtifactCandidateConfidence;
  kind: ArtifactCandidateKind;
  allowed: boolean;
  blockedReason: string | null;
  toolCallId: string | null;
  toolName: string | null;
  toolCallIndex: number;
  appearanceIndex: number;
}

export interface ToolCallArtifactInput {
  toolCallId: string;
  toolName: string;
  args: Record<string, unknown>;
  result?: string;
  toolCallIndex: number;
}

export interface MessageArtifactsRanking {
  primaryToolCallId: string | null;
  primaryCandidate: ArtifactPathCandidate | null;
  secondaryCandidates: ArtifactPathCandidate[];
  candidatesByToolCallId: Map<string, ArtifactPathCandidate[]>;
  argsByToolCallId: Map<string, Record<string, unknown>>;
}

export interface BuildArtifactsIndexResult {
  byMessageId: Map<string, MessageArtifactsRanking>;
  argsToToolCallId: WeakMap<Record<string, unknown>, string>;
}

const PATH_ARG_KEYS = [
  "path",
  "file_path",
  "filepath",
  "output_path",
  "target",
  "to",
  "destination",
] as const;

const PATH_LIST_ARG_KEYS = ["paths", "files"] as const;

const ROUTE_SEGMENTS = new Set([
  "agents",
  "chat",
  "extensions",
  "home",
  "project",
  "projects",
  "settings",
  "session",
  "skills",
  "tasks",
]);

const FILENAME_SIGNAL_WORDS = [
  "report",
  "summary",
  "result",
  "output",
  "final",
  "analysis",
];

function shortToolName(toolName: string): string {
  const normalized = toolName.trim().toLowerCase();
  if (normalized.includes(" ")) {
    return normalized;
  }
  if (normalized.includes("__")) {
    return normalized.slice(normalized.lastIndexOf("__") + 2);
  }
  if (normalized.includes(".")) {
    return normalized.slice(normalized.lastIndexOf(".") + 1);
  }
  return normalized;
}

export function isWriteOrientedTool(toolName: string): boolean {
  const normalized = toolName.trim().toLowerCase();
  if (
    /^(write|writing|create|creating|save|saving|edit|editing|update|updating|modify|modifying|patch|patching)\b/.test(
      normalized,
    )
  ) {
    return true;
  }
  const shortName = shortToolName(toolName);
  return (
    shortName.includes("write_file") ||
    shortName.includes("create_file") ||
    shortName.includes("save_file") ||
    shortName.includes("edit_file") ||
    shortName.includes("update_file") ||
    shortName.includes("modify_file")
  );
}

export function sourceRank(source: ArtifactCandidateSource): number {
  switch (source) {
    case "arg_key":
      return 3;
    case "markdown_href":
      return 2;
    case "result_regex":
      return 1;
    default:
      return 0;
  }
}

export function confidenceRank(
  confidence: ArtifactCandidateConfidence,
): number {
  return confidence === "high" ? 2 : 1;
}

export function normalizePath(path: string): string {
  return path.replace(/\\/g, "/").trim();
}

export function normalizeComparablePath(path: string): string {
  return normalizePath(path).replace(/\/+$/, "").toLowerCase();
}

function hasKnownScheme(value: string): boolean {
  return /^[a-zA-Z][a-zA-Z\d+.-]*:/.test(value);
}

export function isExternalHref(href?: string): boolean {
  if (!href) return false;
  const lower = href.trim().toLowerCase();
  return (
    lower.startsWith("http://") ||
    lower.startsWith("https://") ||
    lower.startsWith("mailto:") ||
    lower.startsWith("tel:")
  );
}

function isLikelyAbsoluteFilesystemPath(candidate: string): boolean {
  if (/^[a-zA-Z]:[\\/]/.test(candidate)) return true;
  if (!candidate.startsWith("/")) return false;
  const firstSegment = candidate.replace(/^\/+/, "").split("/")[0];
  if (!firstSegment) return true;
  if (ROUTE_SEGMENTS.has(firstSegment.toLowerCase())) return false;
  return true;
}

function isLikelyLocalPath(candidate: string): boolean {
  if (!candidate) return false;
  if (candidate === "." || candidate === "..") return false;
  if (candidate.includes("<") || candidate.includes(">")) return false;
  if (/^<\/?[a-zA-Z][^>]*>$/.test(candidate)) return false;
  if (candidate.startsWith("~/")) return true;
  if (candidate.startsWith("./") || candidate.startsWith("../")) return true;
  if (candidate.startsWith("artifacts/") || candidate.startsWith("output/")) {
    return true;
  }
  if (isLikelyAbsoluteFilesystemPath(candidate)) return true;
  if (candidate.includes("/") || candidate.includes("\\")) return true;
  const lastSegment = candidate.split(/[\\/]/).pop() ?? "";
  return /\.[a-zA-Z0-9]{1,12}$/.test(lastSegment);
}

function basename(path: string): string {
  const normalized = normalizePath(path);
  const parts = normalized.split("/").filter(Boolean);
  return parts[parts.length - 1] ?? normalized;
}

function hasExtension(path: string): boolean {
  const name = basename(path);
  const dot = name.lastIndexOf(".");
  return dot > 0 && dot < name.length - 1;
}

function inferPathKind(rawPath: string): ArtifactCandidateKind {
  const normalized = normalizePath(rawPath);
  if (normalized.endsWith("/")) return "folder";
  if (hasExtension(normalized)) return "file";
  return "path";
}

function stripTokenPunctuation(token: string): string {
  return token.replace(/^[([{"'`]+/, "").replace(/[)\]}"'`.,;:!?]+$/, "");
}

function splitIntoTokens(result: string): string[] {
  return result.split(/\s+/).map(stripTokenPunctuation).filter(Boolean);
}

function extractResultPathCandidates(result: string): string[] {
  const tokens = splitIntoTokens(result);
  const matches: string[] = [];
  for (const token of tokens) {
    if (hasKnownScheme(token) && !token.startsWith("file://")) continue;
    if (!isLikelyLocalPath(token)) continue;
    matches.push(token);
  }
  return matches;
}

export function inferHomeDirFromRoots(allowedRoots: string[]): string | null {
  for (const root of allowedRoots) {
    const normalized = normalizePath(root);
    const usersMatch = normalized.match(/^\/Users\/[^/]+/);
    if (usersMatch) return usersMatch[0];
    const homeMatch = normalized.match(/^\/home\/[^/]+/);
    if (homeMatch) return homeMatch[0];
  }
  return null;
}

function resolveRelativeToBase(base: string, relativePath: string): string {
  const normalizedBase = normalizePath(base).replace(/\/+$/, "");
  const normalizedRelative = normalizePath(relativePath).replace(/^\.\/+/, "");
  if (!normalizedRelative || normalizedRelative === ".") return normalizedBase;

  const stack = normalizedBase.split("/").filter(Boolean);
  const hasWindowsDriveRoot = /^[a-zA-Z]:$/.test(stack[0] ?? "");
  for (const segment of normalizedRelative.split("/")) {
    if (!segment || segment === ".") continue;
    if (segment === "..") {
      if (stack.length > 0) stack.pop();
      continue;
    }
    stack.push(segment);
  }

  const resolved = stack.join("/");
  if (hasWindowsDriveRoot) return resolved;
  return `/${resolved}`;
}

function pickBaseRoot(allowedRoots: string[]): string | null {
  const normalizedRoots = allowedRoots
    .map((root) => normalizePath(root))
    .filter(Boolean);
  if (normalizedRoots.length === 0) return null;
  const projectRoots = normalizedRoots.filter(
    (root) => !root.includes("/.goose/artifacts"),
  );
  return projectRoots[0] ?? normalizedRoots[0];
}

export function resolvePathCandidate(
  rawPath: string,
  allowedRoots: string[],
): string {
  const normalizedRaw = normalizePath(rawPath);
  if (!normalizedRaw) return "";

  const homeDir = inferHomeDirFromRoots(allowedRoots);
  if (normalizedRaw.startsWith("~/")) {
    if (!homeDir) return normalizedRaw;
    return `${homeDir}${normalizedRaw.slice(1)}`;
  }

  if (isLikelyAbsoluteFilesystemPath(normalizedRaw)) {
    return normalizedRaw;
  }

  const baseRoot = pickBaseRoot(allowedRoots);
  if (!baseRoot) return normalizedRaw;
  return resolveRelativeToBase(baseRoot, normalizedRaw);
}

export function evaluatePathScope(
  resolvedPath: string,
  allowedRoots: string[],
  options?: { allowOutsideRoots?: boolean },
): { allowed: boolean; blockedReason: string | null } {
  if (options?.allowOutsideRoots) {
    return { allowed: true, blockedReason: null };
  }

  const normalizedPath = normalizeComparablePath(resolvedPath);
  const roots = allowedRoots
    .map((root) => normalizeComparablePath(root))
    .filter(Boolean);
  for (const root of roots) {
    if (normalizedPath === root || normalizedPath.startsWith(`${root}/`)) {
      return { allowed: true, blockedReason: null };
    }
  }
  return {
    allowed: false,
    blockedReason: "Path is outside allowed project/artifacts roots.",
  };
}

function shouldAllowOutsideRootsForToolCandidate(
  input: ToolCallArtifactInput,
  candidate: {
    fromResultText: boolean;
  },
): boolean {
  return isWriteOrientedTool(input.toolName) && candidate.fromResultText;
}

function collectArgPathCandidates(args: Record<string, unknown>): string[] {
  const values: string[] = [];
  for (const key of PATH_ARG_KEYS) {
    const value = args[key];
    if (typeof value === "string" && value.trim()) {
      values.push(value.trim());
    }
  }
  for (const key of PATH_LIST_ARG_KEYS) {
    const value = args[key];
    if (!Array.isArray(value)) continue;
    for (const item of value) {
      if (typeof item === "string" && item.trim()) {
        values.push(item.trim());
      }
    }
  }
  return values;
}

export function filenameSignalBoost(path: string): number {
  const lower = basename(path).toLowerCase();
  return FILENAME_SIGNAL_WORDS.some((word) => lower.includes(word)) ? 1 : 0;
}

export function outputPathSignalBoost(path: string): number {
  const normalized = normalizePath(path).toLowerCase();
  return normalized.includes("/output/") || normalized.startsWith("output/")
    ? 1
    : 0;
}

function buildCandidateId(candidate: {
  toolCallId: string | null;
  source: ArtifactCandidateSource;
  resolvedPath: string;
  appearanceIndex: number;
}): string {
  return [
    candidate.toolCallId ?? "markdown",
    candidate.source,
    normalizeComparablePath(candidate.resolvedPath),
    candidate.appearanceIndex,
  ].join(":");
}

export function extractToolCallCandidates(
  input: ToolCallArtifactInput,
  allowedRoots: string[],
  startingAppearanceIndex = 0,
): ArtifactPathCandidate[] {
  const rawCandidates: Array<{
    rawPath: string;
    source: ArtifactCandidateSource;
    confidence: ArtifactCandidateConfidence;
    fromResultText: boolean;
  }> = [];

  for (const rawPath of collectArgPathCandidates(input.args)) {
    rawCandidates.push({
      rawPath,
      source: "arg_key",
      confidence: "high",
      fromResultText: false,
    });
  }

  for (const candidate of extractToolNamePathCandidates(
    input.toolName,
    isLikelyLocalPath,
    stripTokenPunctuation,
  )) {
    rawCandidates.push(candidate);
  }

  for (const rawPath of collectCommandArgPathCandidates(
    input.args,
    isLikelyLocalPath,
    stripTokenPunctuation,
  )) {
    rawCandidates.push({
      rawPath,
      source: "result_regex",
      confidence: "low",
      fromResultText: false,
    });
  }

  if (
    isWriteOrientedTool(input.toolName) &&
    typeof input.result === "string" &&
    input.result.trim()
  ) {
    for (const rawPath of extractResultPathCandidates(input.result)) {
      rawCandidates.push({
        rawPath,
        source: "result_regex",
        confidence: "low",
        fromResultText: true,
      });
    }
  }

  return rawCandidates.map((raw, idx) => {
    const resolvedPath = resolvePathCandidate(raw.rawPath, allowedRoots);
    const allowOutsideRoots = shouldAllowOutsideRootsForToolCandidate(
      input,
      raw,
    );
    const { allowed, blockedReason } = evaluatePathScope(
      resolvedPath,
      allowedRoots,
      { allowOutsideRoots },
    );
    const appearanceIndex = startingAppearanceIndex + idx;
    return {
      id: buildCandidateId({
        toolCallId: input.toolCallId,
        source: raw.source,
        resolvedPath,
        appearanceIndex,
      }),
      rawPath: raw.rawPath,
      resolvedPath,
      source: raw.source,
      confidence: raw.confidence,
      kind: inferPathKind(raw.rawPath),
      allowed,
      blockedReason,
      toolCallId: input.toolCallId,
      toolName: input.toolName,
      toolCallIndex: input.toolCallIndex,
      appearanceIndex,
    };
  });
}

export function resolveMarkdownLocalHref(
  href: string,
  allowedRoots: string[],
): ArtifactPathCandidate | null {
  const trimmed = href.trim();
  if (!trimmed || trimmed.startsWith("#")) return null;
  if (isExternalHref(trimmed)) return null;
  if (trimmed.toLowerCase().startsWith("javascript:")) return null;

  let rawCandidate = trimmed;
  if (trimmed.toLowerCase().startsWith("file://")) {
    rawCandidate = trimmed.slice("file://".length);
  }

  const withoutHash = rawCandidate.split("#")[0];
  const withoutQuery = withoutHash.split("?")[0];
  if (!isLikelyLocalPath(withoutQuery)) return null;

  const resolvedPath = resolvePathCandidate(withoutQuery, allowedRoots);
  const { allowed, blockedReason } = evaluatePathScope(
    resolvedPath,
    allowedRoots,
  );
  return {
    id: buildCandidateId({
      toolCallId: null,
      source: "markdown_href",
      resolvedPath,
      appearanceIndex: 0,
    }),
    rawPath: withoutQuery,
    resolvedPath,
    source: "markdown_href",
    confidence: "high",
    kind: inferPathKind(withoutQuery),
    allowed,
    blockedReason,
    toolCallId: null,
    toolName: null,
    toolCallIndex: -1,
    appearanceIndex: 0,
  };
}
