import { useEffect, useCallback, useMemo, useRef } from "react";
import type { ChatState } from "@/shared/types/chat";
import { isPromiseLike } from "@/shared/lib/isPromiseLike";
import type { ChatAttachmentDraft } from "@/shared/types/messages";
import { useChatStore } from "../stores/chatStore";

function getQueuedMessageKey(
  queuedMessage: {
    text: string;
    personaId?: string;
    attachments?: ChatAttachmentDraft[];
  } | null,
): string | null {
  if (!queuedMessage) {
    return null;
  }

  return JSON.stringify({
    text: queuedMessage.text,
    personaId: queuedMessage.personaId ?? null,
    attachments:
      queuedMessage.attachments?.map((attachment) => ({
        id: attachment.id,
        kind: attachment.kind,
        name: attachment.name,
        path: "path" in attachment ? attachment.path ?? null : null,
      })) ?? [],
  });
}

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
  ) => boolean | Promise<boolean>,
) {
  const queuedMessage = useChatStore(
    (s) => s.queuedMessageBySession[sessionId] ?? null,
  );
  const failedQueueKeyRef = useRef<string | null>(null);
  const queuedMessageKey = useMemo(
    () => getQueuedMessageKey(queuedMessage),
    [queuedMessage],
  );

  useEffect(() => {
    if (queuedMessageKey !== failedQueueKeyRef.current) {
      failedQueueKeyRef.current = null;
    }
  }, [queuedMessageKey]);

  useEffect(() => {
    if (
      chatState !== "idle" ||
      !queuedMessage ||
      !queuedMessageKey ||
      failedQueueKeyRef.current === queuedMessageKey
    ) {
      return;
    }

    const { text, personaId, attachments } = queuedMessage;
    const sendResult = sendMessage(
      text,
      personaId ? { id: personaId } : undefined,
      attachments,
    );

    const finalize = (accepted: boolean | undefined) => {
      const latestQueuedMessage = useChatStore.getState().queuedMessageBySession[
        sessionId
      ] ?? null;
      if (getQueuedMessageKey(latestQueuedMessage) !== queuedMessageKey) {
        return;
      }

      if (accepted === false) {
        failedQueueKeyRef.current = queuedMessageKey;
        return;
      }

      failedQueueKeyRef.current = null;
      useChatStore.getState().dismissQueuedMessage(sessionId);
    };

    if (isPromiseLike<boolean>(sendResult)) {
      void sendResult
        .then((accepted) => finalize(accepted))
        .catch(() => finalize(false));
    } else {
      finalize(sendResult);
    }
  }, [chatState, queuedMessage, queuedMessageKey, sendMessage, sessionId]);

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
