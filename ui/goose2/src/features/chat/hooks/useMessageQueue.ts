import { useEffect, useCallback } from "react";
import type { ChatState } from "@/shared/types/chat";
import type { ChatAttachmentDraft } from "@/shared/types/messages";
import { useChatStore } from "../stores/chatStore";

/**
 * Single-slot message queue that holds one pending message while the agent is
 * busy and auto-sends it when the chat transitions back to idle.
 *
 * State lives in the Zustand store (keyed by session) so it survives tab
 * switches — users can queue a follow-up, navigate away, and come back to
 * find it sent.
 */
export function useMessageQueue(
  sessionId: string,
  chatState: ChatState,
  sendMessage: (
    text: string,
    overridePersona?: { id: string; name?: string },
    attachments?: ChatAttachmentDraft[],
  ) => void,
) {
  const queuedMessage = useChatStore(
    (s) => s.queuedMessageBySession[sessionId] ?? null,
  );

  useEffect(() => {
    if (chatState === "idle" && queuedMessage) {
      const { text, personaId, attachments } = queuedMessage;
      useChatStore.getState().dismissQueuedMessage(sessionId);
      sendMessage(text, personaId ? { id: personaId } : undefined, attachments);
    }
  }, [chatState, queuedMessage, sendMessage, sessionId]);

  const enqueue = useCallback(
    (text: string, personaId?: string, attachments?: ChatAttachmentDraft[]) => {
      useChatStore.getState().enqueueMessage(sessionId, {
        text,
        personaId,
        attachments,
      });
    },
    [sessionId],
  );

  const dismiss = useCallback(() => {
    useChatStore.getState().dismissQueuedMessage(sessionId);
  }, [sessionId]);

  return { queuedMessage, enqueue, dismiss } as const;
}
