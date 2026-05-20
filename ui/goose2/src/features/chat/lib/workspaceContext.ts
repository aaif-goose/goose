import type {
  ActiveWorkspace,
  ChatSession,
} from "@/features/chat/stores/chatSessionStore";

interface ResolveInheritedProjectWorkspaceOptions {
  projectId: string | undefined;
  sessions: ChatSession[];
  activeSessionId: string | null;
  activeWorkspaceBySession: Record<string, ActiveWorkspace>;
}

export function resolveInheritedProjectWorkspace({
  projectId,
  sessions,
  activeSessionId,
  activeWorkspaceBySession,
}: ResolveInheritedProjectWorkspaceOptions): ActiveWorkspace | undefined {
  if (!projectId || !activeSessionId) {
    return undefined;
  }

  const activeSession = sessions.find(
    (session) => session.id === activeSessionId,
  );
  if (activeSession?.projectId !== projectId) {
    return undefined;
  }

  const activeWorkspace = activeWorkspaceBySession[activeSessionId];
  if (activeWorkspace?.path) {
    return activeWorkspace;
  }

  return activeSession.workingDir
    ? { path: activeSession.workingDir, branch: null }
    : undefined;
}
