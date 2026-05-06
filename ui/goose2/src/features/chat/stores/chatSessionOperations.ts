import {
  renameSession,
  updateSessionProject as updateSessionProjectApi,
} from "@/shared/api/acpApi";
import { useChatSessionStore } from "./chatSessionStore";

const latestRenameBySession = new Map<string, number>();
const latestProjectBySession = new Map<string, number>();

export async function updateSessionTitle(
  sessionId: string,
  title: string,
): Promise<void> {
  const requestId = (latestRenameBySession.get(sessionId) ?? 0) + 1;
  latestRenameBySession.set(sessionId, requestId);

  await renameSession(sessionId, title);

  if (latestRenameBySession.get(sessionId) !== requestId) {
    return;
  }

  useChatSessionStore.getState().patchSession(sessionId, {
    title,
    userSetName: true,
  });
}

export async function updateSessionProject(
  sessionId: string,
  projectId: string | null,
): Promise<void> {
  const requestId = (latestProjectBySession.get(sessionId) ?? 0) + 1;
  latestProjectBySession.set(sessionId, requestId);

  await updateSessionProjectApi(sessionId, projectId);

  if (latestProjectBySession.get(sessionId) !== requestId) {
    return;
  }

  useChatSessionStore.getState().patchSession(sessionId, {
    projectId,
  });
}
