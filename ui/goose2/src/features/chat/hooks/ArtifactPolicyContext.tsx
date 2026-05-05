import { openPath } from "@tauri-apps/plugin-opener";
import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useRef,
  type ReactNode,
} from "react";
import type {
  Message,
  ToolCallLocation,
  ToolKind,
} from "@/shared/types/messages";
import { pathExists } from "@/shared/api/system";

export interface ArtifactLinkCandidate {
  resolvedPath: string;
  rawPath: string;
  line?: number | null;
}

export interface SessionArtifact {
  resolvedPath: string;
  displayPath: string;
  filename: string;
  directoryPath: string;
  resolvedDirectoryPath: string;
  versionCount: number;
  lastTouchedAt: number;
  kind: "file" | "folder" | "path";
  toolName: string | null;
  toolKind?: ToolKind;
  line?: number | null;
}

interface ArtifactPolicyContextValue {
  resolveMarkdownHref: (href: string) => ArtifactLinkCandidate | null;
  pathExists: (path: string) => Promise<boolean>;
  openResolvedPath: (path: string) => Promise<void>;
  getAllSessionArtifacts: () => SessionArtifact[];
}

const DEFAULT_CONTEXT_VALUE: ArtifactPolicyContextValue = {
  resolveMarkdownHref: () => null,
  pathExists: async () => false,
  openResolvedPath: async () => {},
  getAllSessionArtifacts: () => [],
};

const ArtifactPolicyContext = createContext<ArtifactPolicyContextValue>(
  DEFAULT_CONTEXT_VALUE,
);

function normalizePath(path: string): string {
  return path.replace(/\\/g, "/").trim();
}

function normalizeComparablePath(path: string): string {
  return normalizePath(path).replace(/\/+$/, "").toLowerCase();
}

function parentDir(path: string): string {
  const lastSlash = path.lastIndexOf("/");
  if (lastSlash <= 0) return "/";
  return path.slice(0, lastSlash + 1);
}

function basenameOf(path: string): string {
  const parts = path.split("/").filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

function hasExtension(path: string): boolean {
  const name = basenameOf(path);
  const dot = name.lastIndexOf(".");
  return dot > 0 && dot < name.length - 1;
}

function inferPathKind(path: string): SessionArtifact["kind"] {
  const normalized = normalizePath(path);
  if (normalized.endsWith("/")) return "folder";
  if (hasExtension(normalized)) return "file";
  return "path";
}

function isExternalHref(href: string): boolean {
  return (
    /^[a-zA-Z][a-zA-Z\d+.-]*:/.test(href) &&
    !href.toLowerCase().startsWith("file://")
  );
}

function isAbsolutePath(path: string): boolean {
  return path.startsWith("/") || /^[a-zA-Z]:[\\/]/.test(path);
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

function resolvePath(path: string, sessionCwd: string | null): string {
  const normalized = normalizePath(path);
  if (!normalized) return "";

  if (normalized.toLowerCase().startsWith("file://")) {
    return normalized.slice("file://".length);
  }

  if (isAbsolutePath(normalized)) {
    return normalized;
  }

  return sessionCwd
    ? resolveRelativeToBase(sessionCwd, normalized)
    : normalized;
}

function isNonEmptyLocation(
  location: ToolCallLocation,
): location is ToolCallLocation & { path: string } {
  return typeof location.path === "string" && location.path.trim().length > 0;
}

export function ArtifactPolicyProvider({
  messages,
  sessionCwd,
  children,
}: {
  messages: Message[];
  sessionCwd: string | null;
  children: ReactNode;
}) {
  const normalizedSessionCwd = useMemo(
    () => sessionCwd?.trim() || null,
    [sessionCwd],
  );
  const lastOpenAtByPathRef = useRef(new Map<string, number>());

  const resolveMarkdownHref = useCallback(
    (href: string): ArtifactLinkCandidate | null => {
      const trimmed = href.trim();
      if (!trimmed || trimmed.startsWith("#")) return null;
      if (trimmed.toLowerCase().startsWith("javascript:")) return null;
      if (isExternalHref(trimmed)) return null;

      const withoutHash = trimmed.split("#")[0];
      const withoutQuery = withoutHash.split("?")[0];
      if (!withoutQuery) return null;

      return {
        rawPath: withoutQuery,
        resolvedPath: resolvePath(withoutQuery, normalizedSessionCwd),
      };
    },
    [normalizedSessionCwd],
  );

  const resolveOpenTarget = useCallback(
    async (path: string): Promise<string | null> => {
      const resolvedPath = resolvePath(path, normalizedSessionCwd);
      if (await pathExists(resolvedPath)) {
        return resolvedPath;
      }

      return null;
    },
    [normalizedSessionCwd],
  );

  const checkPathExists = useCallback(
    async (path: string) => (await resolveOpenTarget(path)) !== null,
    [resolveOpenTarget],
  );

  const openResolvedPath = useCallback(
    async (path: string) => {
      const resolvedTarget = await resolveOpenTarget(path);
      if (!resolvedTarget) {
        const cwdMessage = normalizedSessionCwd ?? "<none>";
        throw new Error(`File not found: ${path} (session cwd: ${cwdMessage})`);
      }

      const key = resolvedTarget.trim().toLowerCase();
      const now = Date.now();
      const lastOpenAt = lastOpenAtByPathRef.current.get(key) ?? 0;
      if (now - lastOpenAt < 1200) {
        return;
      }
      lastOpenAtByPathRef.current.set(key, now);
      await openPath(resolvedTarget);
    },
    [resolveOpenTarget, normalizedSessionCwd],
  );

  const getAllSessionArtifacts = useCallback((): SessionArtifact[] => {
    const artifactMap = new Map<string, SessionArtifact>();

    for (const message of messages) {
      if (message.role !== "assistant") continue;
      if (message.metadata?.userVisible === false) continue;

      for (const block of message.content) {
        if (block.type !== "toolRequest") continue;
        const locations = block.locations?.filter(isNonEmptyLocation) ?? [];

        for (const location of locations) {
          const resolvedPath = resolvePath(location.path, normalizedSessionCwd);
          const key = normalizeComparablePath(resolvedPath);
          if (!key) continue;

          const existing = artifactMap.get(key);
          if (existing) {
            existing.versionCount += 1;
            if (message.created > existing.lastTouchedAt) {
              existing.lastTouchedAt = message.created;
              existing.toolName = block.toolName ?? block.name;
              existing.toolKind = block.toolKind;
              existing.line = location.line;
            }
            continue;
          }

          artifactMap.set(key, {
            resolvedPath,
            displayPath: resolvedPath,
            filename: basenameOf(resolvedPath),
            directoryPath: parentDir(resolvedPath),
            resolvedDirectoryPath: parentDir(resolvedPath),
            versionCount: 1,
            lastTouchedAt: message.created,
            kind: inferPathKind(resolvedPath),
            toolName: block.toolName ?? block.name,
            toolKind: block.toolKind,
            line: location.line,
          });
        }
      }
    }

    return Array.from(artifactMap.values()).sort(
      (a, b) => b.lastTouchedAt - a.lastTouchedAt,
    );
  }, [messages, normalizedSessionCwd]);

  const contextValue = useMemo<ArtifactPolicyContextValue>(
    () => ({
      resolveMarkdownHref,
      pathExists: checkPathExists,
      openResolvedPath,
      getAllSessionArtifacts,
    }),
    [
      checkPathExists,
      getAllSessionArtifacts,
      openResolvedPath,
      resolveMarkdownHref,
    ],
  );

  return (
    <ArtifactPolicyContext.Provider value={contextValue}>
      {children}
    </ArtifactPolicyContext.Provider>
  );
}

export function useArtifactPolicyContext(): ArtifactPolicyContextValue {
  return useContext(ArtifactPolicyContext);
}
