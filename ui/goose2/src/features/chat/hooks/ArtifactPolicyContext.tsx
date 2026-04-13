import { openPath } from "@tauri-apps/plugin-opener";
import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useRef,
  type ReactNode,
} from "react";
import type { Message } from "@/shared/types/messages";
import { pathExists } from "@/shared/api/system";
import {
  buildArtifactsIndexForMessages,
  inferHomeDirFromRoots,
  isWriteOrientedTool,
  resolveMarkdownLocalHref,
  type ArtifactPathCandidate,
} from "@/features/chat/lib/artifactPathPolicy";

export interface ToolCardDisplay {
  role: "primary_host" | "none";
  primaryCandidate: ArtifactPathCandidate | null;
  secondaryCandidates: ArtifactPathCandidate[];
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
}

interface ArtifactPolicyContextValue {
  resolveToolCardDisplay: (
    args: Record<string, unknown>,
    name: string,
    result?: string,
  ) => ToolCardDisplay;
  resolveMarkdownHref: (href: string) => ArtifactPathCandidate | null;
  pathExists: (path: string) => Promise<boolean>;
  openResolvedPath: (path: string) => Promise<void>;
  getAllSessionArtifacts: () => SessionArtifact[];
}

const EMPTY_DISPLAY: ToolCardDisplay = {
  role: "none",
  primaryCandidate: null,
  secondaryCandidates: [],
};

const DEFAULT_CONTEXT_VALUE: ArtifactPolicyContextValue = {
  resolveToolCardDisplay: () => EMPTY_DISPLAY,
  resolveMarkdownHref: () => null,
  pathExists: async () => false,
  openResolvedPath: async () => {},
  getAllSessionArtifacts: () => [],
};

const ArtifactPolicyContext = createContext<ArtifactPolicyContextValue>(
  DEFAULT_CONTEXT_VALUE,
);

function shortenPath(fullPath: string, homeDir: string | null): string {
  if (homeDir && fullPath.startsWith(homeDir)) {
    return `~${fullPath.slice(homeDir.length)}`;
  }
  return fullPath;
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

function homeArtifactsRootFromRoots(roots: string[]): string | null {
  return (
    roots.find((root) => /\/\.goose\/artifacts\/?$/.test(root.trim())) ?? null
  );
}

function stripRootArtifactsSegment(
  path: string,
  roots: string[],
): string | null {
  for (const root of roots) {
    const normalizedRoot = root.replace(/\/+$/, "");
    const prefix = `${normalizedRoot}/artifacts/`;
    if (path.startsWith(prefix)) {
      return `${normalizedRoot}/${path.slice(prefix.length)}`;
    }
  }
  return null;
}

function fallbackArtifactPath(path: string, roots: string[]): string | null {
  const normalized = path.trim();
  if (!/\/\.goose\/projects\/.+\/artifacts\/.+/.test(normalized)) {
    const rootAnchored = stripRootArtifactsSegment(normalized, roots);
    if (rootAnchored) {
      return rootAnchored;
    }
    return null;
  }

  const filename = basenameOf(normalized);
  if (!filename || filename === normalized) {
    return null;
  }

  const homeArtifactsRoot = homeArtifactsRootFromRoots(roots);
  if (!homeArtifactsRoot) {
    return null;
  }

  return `${homeArtifactsRoot.replace(/\/+$/, "")}/${filename}`;
}

export function ArtifactPolicyProvider({
  messages,
  allowedRoots,
  children,
}: {
  messages: Message[];
  allowedRoots: string[];
  children: ReactNode;
}) {
  const normalizedRoots = useMemo(
    () => [...new Set(allowedRoots.map((root) => root.trim()).filter(Boolean))],
    [allowedRoots],
  );
  const lastOpenAtByPathRef = useRef(new Map<string, number>());

  const artifactsIndex = useMemo(
    () => buildArtifactsIndexForMessages(messages, normalizedRoots),
    [messages, normalizedRoots],
  );

  const { argsToToolCallId, toolCardDisplayByToolCallId } = useMemo(() => {
    const displayByToolCallId = new Map<string, ToolCardDisplay>();

    for (const ranking of artifactsIndex.byMessageId.values()) {
      if (!ranking.primaryToolCallId || !ranking.primaryCandidate) continue;
      if (
        !ranking.primaryCandidate.toolName ||
        !isWriteOrientedTool(ranking.primaryCandidate.toolName)
      ) {
        continue;
      }
      displayByToolCallId.set(ranking.primaryToolCallId, {
        role: "primary_host",
        primaryCandidate: ranking.primaryCandidate,
        secondaryCandidates: ranking.secondaryCandidates,
      });
    }

    return {
      argsToToolCallId: artifactsIndex.argsToToolCallId,
      toolCardDisplayByToolCallId: displayByToolCallId,
    };
  }, [artifactsIndex]);

  const resolveToolCardDisplay = useCallback(
    (args: Record<string, unknown>, _name: string, _result?: string) => {
      const toolCallId = argsToToolCallId.get(args);
      if (!toolCallId) return EMPTY_DISPLAY;
      return toolCardDisplayByToolCallId.get(toolCallId) ?? EMPTY_DISPLAY;
    },
    [argsToToolCallId, toolCardDisplayByToolCallId],
  );

  const resolveMarkdownHref = useCallback(
    (href: string) => resolveMarkdownLocalHref(href, normalizedRoots),
    [normalizedRoots],
  );

  const resolveOpenTarget = useCallback(
    async (path: string): Promise<string | null> => {
      if (await pathExists(path)) {
        return path;
      }

      const fallbackPath = fallbackArtifactPath(path, normalizedRoots);
      if (fallbackPath && (await pathExists(fallbackPath))) {
        return fallbackPath;
      }

      return null;
    },
    [normalizedRoots],
  );

  const checkPathExists = useCallback(
    async (path: string) => (await resolveOpenTarget(path)) !== null,
    [resolveOpenTarget],
  );

  const openResolvedPath = useCallback(
    async (path: string) => {
      const resolvedTarget = await resolveOpenTarget(path);
      if (!resolvedTarget) {
        throw new Error(`File not found: ${path}`);
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
    [resolveOpenTarget],
  );

  const getAllSessionArtifacts = useCallback((): SessionArtifact[] => {
    const homeDir =
      normalizedRoots.length > 0
        ? inferHomeDirFromRoots(normalizedRoots)
        : null;

    const artifactMap = new Map<string, SessionArtifact>();

    for (const [messageId, ranking] of artifactsIndex.byMessageId.entries()) {
      const message = messages.find((m) => m.id === messageId);
      const timestamp = message?.created ?? 0;

      for (const candidates of ranking.candidatesByToolCallId.values()) {
        for (const candidate of candidates) {
          if (!candidate.allowed) continue;
          if (!candidate.toolName || !isWriteOrientedTool(candidate.toolName)) {
            continue;
          }
          const key = candidate.resolvedPath.trim().toLowerCase();
          const existing = artifactMap.get(key);

          if (existing) {
            existing.versionCount += 1;
            if (timestamp > existing.lastTouchedAt) {
              existing.lastTouchedAt = timestamp;
              existing.toolName = candidate.toolName;
            }
          } else {
            artifactMap.set(key, {
              resolvedPath: candidate.resolvedPath,
              displayPath: shortenPath(candidate.resolvedPath, homeDir),
              filename: basenameOf(candidate.resolvedPath),
              directoryPath: shortenPath(
                parentDir(candidate.resolvedPath),
                homeDir,
              ),
              resolvedDirectoryPath: parentDir(candidate.resolvedPath),
              versionCount: 1,
              lastTouchedAt: timestamp,
              kind: candidate.kind,
              toolName: candidate.toolName,
            });
          }
        }
      }
    }

    return Array.from(artifactMap.values()).sort(
      (a, b) => b.lastTouchedAt - a.lastTouchedAt,
    );
  }, [messages, normalizedRoots, artifactsIndex]);

  const contextValue = useMemo<ArtifactPolicyContextValue>(
    () => ({
      resolveToolCardDisplay,
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
      resolveToolCardDisplay,
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
