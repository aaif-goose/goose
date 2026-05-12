import type { SessionUpdate } from "@agentclientprotocol/sdk";
import { useChatSessionStore } from "@/features/chat/stores/chatSessionStore";

type SessionInfoUpdate = SessionUpdate & {
  sessionUpdate: "session_info_update";
  title?: unknown;
  updatedAt?: unknown;
  _meta?: unknown;
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

export function handleSessionInfoUpdate(
  sessionId: string,
  update: SessionUpdate,
): void {
  const info = update as SessionInfoUpdate;
  const sessionStore = useChatSessionStore.getState();
  const session = sessionStore.getSession(sessionId);
  if (!session) {
    return;
  }

  const meta = isRecord(info._meta) ? info._meta : {};
  const patch: Parameters<typeof sessionStore.patchSession>[1] = {};

  if (typeof info.title === "string" && info.title && !session.userSetName) {
    patch.title = info.title;
  }
  if (typeof info.updatedAt === "string" && info.updatedAt) {
    patch.updatedAt = info.updatedAt;
  }
  if (typeof meta.messageCount === "number") {
    patch.messageCount = meta.messageCount;
  }
  if (typeof meta.userSetName === "boolean") {
    patch.userSetName = meta.userSetName;
  }

  if (Object.keys(patch).length > 0) {
    sessionStore.patchSession(sessionId, patch);
  }
}
