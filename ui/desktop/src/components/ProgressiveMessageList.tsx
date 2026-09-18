import { Fragment, memo, useEffect, useMemo, useRef, useState } from 'react';
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
import {
  getToolResponses,
  type ImageData,
  type Message,
  type NotificationEvent,
  type SystemNotificationContent,
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
  messagesOmitted: {
    id: 'progressiveMessageList.messagesOmitted',
    defaultMessage:
      '{count, plural, one {# earlier message omitted} other {# earlier messages omitted}}',
  },
  showAllMessages: {
    id: 'progressiveMessageList.showAllMessages',
    defaultMessage: 'Show all',
  },
});

export const MESSAGE_DISPLAY_LIMIT = 250;
export const HEAD_MESSAGE_COUNT = 10;
export const TAIL_MESSAGE_COUNT = 190;

interface DisplayWindow {
  displayed: Message[];
  headLength: number;
  omitted: number;
}

function startsTurn(message: Message): boolean {
  return (
    message.role === 'user' &&
    message.metadata.userVisible &&
    getToolResponses(message).length === 0
  );
}

function everyMessage(messages: Message[]): DisplayWindow {
  return { displayed: messages, headLength: messages.length, omitted: 0 };
}

/**
 * Long conversations render only their opening and their most recent turns.
 * Both edges are snapped to a turn boundary so a tool request is never
 * separated from the response that arrives in a later message.
 */
function displayWindow(messages: Message[]): DisplayWindow {
  if (messages.length <= MESSAGE_DISPLAY_LIMIT) {
    return everyMessage(messages);
  }

  let headEnd = HEAD_MESSAGE_COUNT;
  while (headEnd < messages.length && !startsTurn(messages[headEnd])) {
    headEnd += 1;
  }

  let tailStart = messages.length - TAIL_MESSAGE_COUNT;
  while (tailStart > 0 && !startsTurn(messages[tailStart])) {
    tailStart -= 1;
  }

  if (tailStart <= headEnd) {
    return everyMessage(messages);
  }

  return {
    displayed: [...messages.slice(0, headEnd), ...messages.slice(tailStart)],
    headLength: headEnd,
    omitted: tailStart - headEnd,
  };
}

const emptyToolCallNotifications = new Map<string, NotificationEvent[]>();
const emptyAppend = () => {};

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
  messages: allMessages,
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
  const [showAllMessages, setShowAllMessages] = useState(false);
  const {
    displayed: messages,
    headLength,
    omitted,
  } = useMemo(
    () => (showAllMessages ? everyMessage(allMessages) : displayWindow(allMessages)),
    [allMessages, showAllMessages]
  );
  const [renderedCount, setRenderedCount] = useState(() =>
    messages.length <= showLoadingThreshold ? messages.length : Math.min(batchSize, messages.length)
  );
  const completedMessageKeyRef = useRef<string | null>(null);
  const isLoading = renderedCount < messages.length;

  useEffect(() => {
    if (messages.length <= showLoadingThreshold) {
      setRenderedCount(messages.length);
      return;
    }

    if (!isLoading) return;

    const timeout = window.setTimeout(() => {
      setRenderedCount((current) => Math.min(current + batchSize, messages.length));
    }, batchDelay);

    return () => window.clearTimeout(timeout);
  }, [batchDelay, batchSize, isLoading, messages.length, renderedCount, showLoadingThreshold]);

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
    if (!isLoading && omitted === 0) return;

    const handleKeyDown = (event: KeyboardEvent) => {
      const isMac = window.electron.platform === 'darwin';
      const isSearchShortcut = (isMac ? event.metaKey : event.ctrlKey) && event.key === 'f';

      if (isSearchShortcut) {
        setShowAllMessages(true);
        setRenderedCount(allMessages.length);
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isLoading, omitted, allMessages.length]);

  const rowContexts = useMemo(() => deriveMessageRowContexts(messages), [messages]);
  const messagesToRender = messages.slice(0, renderedCount);
  const messageRows = messagesToRender
    .map((message, index) => {
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

      const row = (
        <MessageRow
          key={messageKey}
          append={append}
          index={index}
          isStreaming={
            isStreamingMessage &&
            !isUser &&
            index === messagesToRender.length - 1 &&
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

      if (omitted === 0 || index !== headLength) return row;

      return (
        <Fragment key={messageKey}>
          <div className="flex flex-wrap items-center justify-center gap-2 my-4 text-xs text-text-muted">
            <span>{intl.formatMessage(i18n.messagesOmitted, { count: omitted })}</span>
            <button
              type="button"
              className="underline hover:text-text-standard"
              onClick={() => setShowAllMessages(true)}
            >
              {intl.formatMessage(i18n.showAllMessages)}
            </button>
          </div>
          {row}
        </Fragment>
      );
    })
    .filter(Boolean);

  return (
    <>
      {messageRows}

      {isLoading && (
        <div className="flex flex-col items-center justify-center py-8">
          <LoadingGoose
            message={intl.formatMessage(i18n.loadingMessages, {
              renderedCount,
              totalCount: messages.length,
            })}
          />
          <div className="text-xs text-text-secondary mt-2">
            {intl.formatMessage(i18n.searchHint)}
          </div>
        </div>
      )}
    </>
  );
}
