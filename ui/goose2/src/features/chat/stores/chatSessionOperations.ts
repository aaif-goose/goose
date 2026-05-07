import {
  renameSession,
  updateSessionProject as updateSessionProjectApi,
} from "@/shared/api/acpApi";
import { useChatSessionStore } from "./chatSessionStore";

export async function updateSessionTitle(
  sessionId: string,
  title: string,
): Promise<void> {
  await renameSession(sessionId, title);

  useChatSessionStore.getState().patchSession(sessionId, {
    title,
    userSetName: true,
  });
}

export async function updateSessionProject(
  sessionId: string,
  projectId: string | null,
): Promise<void> {
  await updateSessionProjectApi(sessionId, projectId);

  useChatSessionStore.getState().patchSession(sessionId, {
    projectId,
  });
}
