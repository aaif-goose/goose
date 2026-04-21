import type { Message } from "@/shared/types/messages";
import type { ChatSession } from "../stores/chatSessionStore";
import { DEFAULT_CHAT_TITLE } from "./sessionTitle";

interface NewChatRequest {
  title: string;
  projectId?: string;
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
  return session.projectId === request.projectId;
}

function isReusableDraft(
  session: ChatSession,
  _localMessages: Message[] | undefined,
): boolean {
  return !session.archivedAt && session.messageCount === 0;
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
    (session) => (draftsBySession[session.id] ?? "").length > 0,
  );
  if (withContent.length > 0) {
    return (
      withContent.find((session) => session.id === activeSessionId) ??
      withContent[0]
    );
  }

  return candidates.find((session) => session.id === activeSessionId);
}
