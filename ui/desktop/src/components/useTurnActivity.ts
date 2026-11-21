import { useMemo } from 'react';
import type { Message } from '../types/message';
import { deriveMessageRowContexts } from './messageRowContext';
import { useShowThinking } from '../hooks/useShowThinking';
import { deriveTurnActivity } from './turnActivity';

export function useTurnActivity(
  messages: Message[],
  isUserMessage: (message: Message) => boolean,
  isActive: boolean
) {
  const showThinking = useShowThinking();
  const visibleMessages = useMemo(
    () =>
      messages.flatMap((message, index) =>
        message.metadata.userVisible ? [{ message, index }] : []
      ),
    [messages]
  );
  const rowContexts = useMemo(() => deriveMessageRowContexts(messages), [messages]);
  const turns = useMemo(
    () =>
      deriveTurnActivity(
        visibleMessages,
        rowContexts,
        isUserMessage,
        isActive,
        showThinking !== null && showThinking !== 'never'
      ),
    [visibleMessages, rowContexts, isUserMessage, isActive, showThinking]
  );
  return { visibleMessages, rowContexts, turns };
}
