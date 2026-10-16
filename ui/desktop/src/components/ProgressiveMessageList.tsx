import { Fragment, memo, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { flushSync } from 'react-dom';
import { isEqual } from 'lodash';
import { defineMessages, useIntl } from '../i18n';
import GooseMessage from './GooseMessage';
import UserMessage from './UserMessage';
import {
  SystemNotificationInline,
  getInlineSystemNotification,
} from './context_management/SystemNotificationInline';
import {
  CreditsExhaustedNotification,
  getCreditsExhaustedNotification,
} from './context_management/CreditsExhaustedNotification';
import type {
  ImageData,
  Message,
  NotificationEvent,
  SystemNotificationContent,
} from '../types/message';
import LoadingGoose from './LoadingGoose';
import { getModelDisplayName } from './settings/models/predefinedModelsUtils';
import { deriveMessageRowContexts, type MessageRowContext } from './messageRowContext';

const i18n = defineMessages({
  loadingMessages: {
    id: 'progressiveMessageList.loadingMessages',
    defaultMessage: 'Loading messages... ({renderedCount}/{totalCount})',
  },
  searchHint: {
    id: 'progressiveMessageList.searchHint',
    defaultMessage: 'Press Cmd/Ctrl+F to load all messages immediately for search',
  },
  modelChanged: {
    id: 'progressiveMessageList.modelChanged',
    defaultMessage: 'Model changed: {previousModel} → {currentModel}',
  },
});

const emptyToolCallNotifications = new Map<string, NotificationEvent[]>();
const emptyAppend = () => {};

function getInitialFirstRenderedIndex(
  messageCount: number,
  batchSize: number,
  showLoadingThreshold: number
): number {
  return messageCount <= showLoadingThreshold ? 0 : Math.max(0, messageCount - batchSize);
}

function getResolvedModel(message: Message): string | null {
  if (message.role !== 'assistant' || !message.metadata.userVisible) return null;
  return message.metadata.inference?.resolvedModel ?? null;
}

function getSystemNotification(message: Message): SystemNotificationContent | undefined {
  return getCreditsExhaustedNotification(message) ?? getInlineSystemNotification(message);
}

function renderSystemNotification(notification: SystemNotificationContent) {
  switch (notification.notificationType) {
    case 'creditsExhausted':
      return <CreditsExhaustedNotification notification={notification} />;
    case 'inlineMessage':
      return <SystemNotificationInline notification={notification} />;
    default:
      return null;
  }
}

interface MessageRowProps {
  append: (value: string) => void;
  index: number;
  isStreaming: boolean;
  isUser: boolean;
  message: Message;
  modelChangeMessage: string | null;
  onMessageUpdate?: (
    messageId: string,
    newContent: string,
    editType: 'fork' | 'edit',
    retainedImages: ImageData[]
  ) => void;
  rowContext: MessageRowContext;
  sessionId: string;
  submitElicitationResponse?: (
    elicitationId: string,
    userData: Record<string, unknown>
  ) => Promise<boolean>;
  toolNotifications: readonly (NotificationEvent[] | undefined)[];
}

function MessageRowComponent({
  append,
  index,
  isStreaming,
  isUser,
  message,
  modelChangeMessage,
  onMessageUpdate,
  rowContext,
  sessionId,
  submitElicitationResponse,
  toolNotifications,
}: MessageRowProps) {
  const notification = getSystemNotification(message);

  if (notification) {
    return (
      <div
        className={`relative ${index === 0 ? 'mt-0' : 'mt-4'} assistant`}
        data-testid="message-container"
      >
        {renderSystemNotification(notification)}
      </div>
    );
  }

  const hasOnlyToolResponses = message.content.every((content) => content.type === 'toolResponse');

  return (
    <Fragment>
      {modelChangeMessage && (
        <SystemNotificationInline
          notification={{
            msg: modelChangeMessage,
            notificationType: 'inlineMessage',
          }}
        />
      )}
      <div
        className={`relative ${index === 0 ? 'mt-0' : 'mt-4'} ${isUser ? 'user' : 'assistant'} ${rowContext.isInToolCallChain ? 'in-chain' : ''}`}
        data-testid="message-container"
      >
        {isUser ? (
          !hasOnlyToolResponses && (
            <UserMessage message={message} onMessageUpdate={onMessageUpdate} />
          )
        ) : (
          <GooseMessage
            sessionId={sessionId}
            message={message}
            hideTimestamp={rowContext.hideTimestamp}
            toolStates={rowContext.toolStates}
            toolNotifications={toolNotifications}
            toolConfirmationShownInline={rowContext.toolConfirmationShownInline}
            append={append}
            isStreaming={isStreaming}
            submitElicitationResponse={submitElicitationResponse}
          />
        )}
      </div>
    </Fragment>
  );
}

const MessageRow = memo(MessageRowComponent, isEqual);

interface ProgressiveMessageListProps {
  messages: Message[];
  sessionId: string;
  toolCallNotifications?: Map<string, NotificationEvent[]>;
  append?: (value: string) => void;
  isUserMessage: (message: Message) => boolean;
  batchSize?: number;
  batchDelay?: number;
  showLoadingThreshold?: number;
  renderMessage?: (message: Message, index: number) => React.ReactNode | null;
  isStreamingMessage?: boolean;
  onMessageUpdate?: (
    messageId: string,
    newContent: string,
    editType: 'fork' | 'edit',
    retainedImages: ImageData[]
  ) => void;
  onRenderingComplete?: () => void;
  submitElicitationResponse?: (
    elicitationId: string,
    userData: Record<string, unknown>
  ) => Promise<boolean>;
}

export default function ProgressiveMessageList({
  messages,
  sessionId,
  toolCallNotifications = emptyToolCallNotifications,
  append = emptyAppend,
  isUserMessage,
  batchSize = 5,
  batchDelay = 20,
  showLoadingThreshold = 50,
  renderMessage,
  isStreamingMessage = false,
  onMessageUpdate,
  onRenderingComplete,
  submitElicitationResponse,
}: ProgressiveMessageListProps) {
  const intl = useIntl();
  // Batches reveal older history above the newest messages. Messages that arrive while
  // older batches load are appended below, so the rows already on screen stay in place.
  const [firstRenderedIndex, setFirstRenderedIndex] = useState(() =>
    getInitialFirstRenderedIndex(messages.length, batchSize, showLoadingThreshold)
  );
  // A different session, or a conversation that reloads into an empty list, starts again
  // from its newest messages.
  const conversationKey = messages.length > 0 ? sessionId : null;
  const [renderedConversationKey, setRenderedConversationKey] = useState(conversationKey);
  if (conversationKey !== renderedConversationKey) {
    setRenderedConversationKey(conversationKey);
    setFirstRenderedIndex(
      getInitialFirstRenderedIndex(messages.length, batchSize, showLoadingThreshold)
    );
  }
  const loadingBannerRef = useRef<HTMLDivElement>(null);
  const completedMessageKeyRef = useRef<string | null>(null);
  const isLoading = firstRenderedIndex > 0;

  const renderMessagesFrom = useCallback((index: number) => {
    // Older rows go in between the loading banner and the rows on screen. The browser does
    // not keep the view in place for that while the banner is visible, so move the scroll
    // position by however far the first row on screen was pushed down. The render is
    // flushed synchronously so that a user scroll cannot land between the two measurements.
    const firstRow = loadingBannerRef.current?.nextElementSibling;
    const scrollContainer = firstRow?.closest('[data-radix-scroll-area-viewport]');
    if (!firstRow || !scrollContainer) {
      setFirstRenderedIndex(index);
      return;
    }

    const topBefore = firstRow.getBoundingClientRect().top;
    flushSync(() => setFirstRenderedIndex(index));
    scrollContainer.scrollTop += firstRow.getBoundingClientRect().top - topBefore;
  }, []);

  useEffect(() => {
    if (!isLoading) return;

    const timeout = window.setTimeout(() => {
      renderMessagesFrom(Math.max(0, firstRenderedIndex - batchSize));
    }, batchDelay);

    return () => window.clearTimeout(timeout);
  }, [batchDelay, batchSize, firstRenderedIndex, isLoading, renderMessagesFrom]);

  useEffect(() => {
    if (isLoading) return;

    const completedMessageKey = `${sessionId}:${messages.length}`;
    if (completedMessageKeyRef.current === completedMessageKey) return;

    const timeout = window.setTimeout(() => {
      completedMessageKeyRef.current = completedMessageKey;
      onRenderingComplete?.();
    }, 50);

    return () => window.clearTimeout(timeout);
  }, [isLoading, messages.length, onRenderingComplete, sessionId]);

  useEffect(() => {
    if (!isLoading) return;

    const handleKeyDown = (event: KeyboardEvent) => {
      const isMac = window.electron.platform === 'darwin';
      const isSearchShortcut = (isMac ? event.metaKey : event.ctrlKey) && event.key === 'f';

      if (isSearchShortcut) {
        renderMessagesFrom(0);
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isLoading, renderMessagesFrom]);

  const rowContexts = useMemo(() => deriveMessageRowContexts(messages), [messages]);
  const messageRows = messages
    .slice(firstRenderedIndex)
    .map((message, renderedIndex) => {
      const index = firstRenderedIndex + renderedIndex;
      if (!message.metadata.userVisible) return null;
      if (renderMessage) return renderMessage(message, index);

      const isUser = isUserMessage(message);
      const messageIdentifier = message.id ?? `msg-${index}-${message.created}`;
      const messageKey = getSystemNotification(message)
        ? `notification-${messageIdentifier}`
        : messageIdentifier;
      const rowContext = rowContexts[index];
      const currentResolvedModel = getResolvedModel(message);
      const modelChangeMessage =
        currentResolvedModel &&
        rowContext.previousResolvedModel &&
        currentResolvedModel !== rowContext.previousResolvedModel
          ? intl.formatMessage(i18n.modelChanged, {
              previousModel: getModelDisplayName(rowContext.previousResolvedModel),
              currentModel: getModelDisplayName(currentResolvedModel),
            })
          : null;
      const toolNotifications = rowContext.toolStates.map((toolState) =>
        toolCallNotifications.get(toolState.requestId)
      );

      return (
        <MessageRow
          key={messageKey}
          append={append}
          index={index}
          isStreaming={
            isStreamingMessage &&
            !isUser &&
            index === messages.length - 1 &&
            message.role === 'assistant'
          }
          isUser={isUser}
          message={message}
          modelChangeMessage={modelChangeMessage}
          onMessageUpdate={onMessageUpdate}
          rowContext={rowContext}
          sessionId={sessionId}
          submitElicitationResponse={submitElicitationResponse}
          toolNotifications={toolNotifications}
        />
      );
    })
    .filter(Boolean);

  return (
    <>
      {isLoading && (
        <div ref={loadingBannerRef} className="flex flex-col items-center justify-center py-8">
          <LoadingGoose
            message={intl.formatMessage(i18n.loadingMessages, {
              renderedCount: messages.length - firstRenderedIndex,
              totalCount: messages.length,
            })}
          />
          <div className="text-xs text-text-secondary mt-2">
            {intl.formatMessage(i18n.searchHint)}
          </div>
        </div>
      )}

      {messageRows}
    </>
  );
}
