import type { Message } from "@/shared/types/messages";
import type { ChatSession } from "../stores/chatSessionStore";
import { DEFAULT_CHAT_TITLE } from "./sessionTitle";

interface NewChatRequest {
  title: string;
  projectId?: string;
  agentId?: string;
  providerId?: string;
  personaId?: string;
}

interface FindExistingDraftArgs {
  sessions: ChatSession[];
  activeSessionId: string | null;
  draftsBySession: Record<string, string>;
  messagesBySession: Record<string, Message[]>;
  request: NewChatRequest;
}

function isMatchingContext(
  session: ChatSession,
  request: Omit<NewChatRequest, "title">,
): boolean {
  return (
    session.projectId === request.projectId &&
    session.agentId === request.agentId &&
    session.providerId === request.providerId &&
    session.personaId === request.personaId
  );
}

function isReusableDraft(
  session: ChatSession,
  localMessages: Message[] | undefined,
): boolean {
  return (
    !!session.draft &&
    !session.archivedAt &&
    session.messageCount === 0 &&
    (localMessages?.length ?? 0) === 0
  );
}

export function findExistingDraft({
  sessions,
  activeSessionId,
  draftsBySession,
  messagesBySession,
  request,
}: FindExistingDraftArgs): ChatSession | undefined {
  if (request.title !== DEFAULT_CHAT_TITLE) {
    return undefined;
  }

  const candidates = sessions.filter(
    (session) =>
      isMatchingContext(session, request) &&
      isReusableDraft(session, messagesBySession[session.id]),
  );

  if (candidates.length === 0) {
    return undefined;
  }

  const withContent = candidates.filter(
    (s) => (draftsBySession[s.id] ?? "").length > 0,
  );
  if (withContent.length > 0) {
    return withContent.find((s) => s.id === activeSessionId) ?? withContent[0];
  }

  const active = candidates.find((s) => s.id === activeSessionId);
  if (active) {
    return active;
  }

  return candidates.sort(
    (a, b) => new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime(),
  )[0];
}
