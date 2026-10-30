import { Fragment, memo, useEffect, useMemo, useState } from 'react';
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
import { getModelDisplayName } from './settings/models/predefinedModelsUtils';
import {
  deriveMessageRowContexts,
  type InferenceModel,
  inferenceModel,
  isModelChange,
  type MessageRowContext,
} from './messageRowContext';

const i18n = defineMessages({
  modelChanged: {
    id: 'messageList.modelChanged',
    defaultMessage: 'Model changed: {previousModel} → {currentModel}',
  },
  requestedModel: {
    id: 'messageList.requestedModel',
    defaultMessage: '{model} (requested)',
  },
  hiddenMessages: {
    id: 'messageList.hiddenMessages',
    defaultMessage: '{count, plural, one {# message hidden} other {# messages hidden}}',
  },
  showAllMessages: {
    id: 'messageList.showAllMessages',
    defaultMessage: 'Show all',
  },
});

const emptyToolCallNotifications = new Map<string, NotificationEvent[]>();
const emptyAppend = () => {};
const HEAD_MESSAGE_COUNT = 20;
const TAIL_MESSAGE_COUNT = 200;
const MESSAGE_DISPLAY_LIMIT = HEAD_MESSAGE_COUNT + TAIL_MESSAGE_COUNT;

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

interface MessageListProps {
  messages: Message[];
  sessionId: string;
  toolCallNotifications?: Map<string, NotificationEvent[]>;
  append?: (value: string) => void;
  isUserMessage: (message: Message) => boolean;
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

export default function MessageList({
  messages,
  sessionId,
  toolCallNotifications = emptyToolCallNotifications,
  append = emptyAppend,
  isUserMessage,
  isStreamingMessage = false,
  onMessageUpdate,
  onRenderingComplete,
  submitElicitationResponse,
}: MessageListProps) {
  const intl = useIntl();
  const [showAllMessages, setShowAllMessages] = useState(false);
  const visibleMessages = useMemo(
    () =>
      messages.flatMap((message, index) =>
        message.metadata.userVisible ? [{ message, index }] : []
      ),
    [messages]
  );
  const hiddenMessageCount = showAllMessages
    ? 0
    : Math.max(0, visibleMessages.length - MESSAGE_DISPLAY_LIMIT);
  const hasHiddenMessages = hiddenMessageCount > 0;
  const displayedMessages = useMemo(() => {
    if (!hasHiddenMessages) {
      return visibleMessages;
    }

    const tailStart = visibleMessages.length - TAIL_MESSAGE_COUNT;
    return [...visibleMessages.slice(0, HEAD_MESSAGE_COUNT), ...visibleMessages.slice(tailStart)];
  }, [hasHiddenMessages, visibleMessages]);

  useEffect(() => {
    onRenderingComplete?.();
  }, [messages.length, onRenderingComplete, sessionId]);

  useEffect(() => {
    if (!hasHiddenMessages) return;

    const handleKeyDown = (event: KeyboardEvent) => {
      const isMac = window.electron.platform === 'darwin';
      const isSearchShortcut =
        (isMac ? event.metaKey : event.ctrlKey) && !event.shiftKey && event.key === 'f';

      if (isSearchShortcut) {
        setShowAllMessages(true);
      }
    };
    const handleFindCommand = () => setShowAllMessages(true);

    window.addEventListener('keydown', handleKeyDown);
    window.electron.on('find-command', handleFindCommand);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
      window.electron.off('find-command', handleFindCommand);
    };
  }, [hasHiddenMessages]);

  const rowContexts = useMemo(() => deriveMessageRowContexts(messages), [messages]);
  const modelLabel = (model: InferenceModel) =>
    model.confirmed
      ? getModelDisplayName(model.id)
      : intl.formatMessage(i18n.requestedModel, { model: getModelDisplayName(model.id) });
  const messageRows: React.ReactNode[] = [];

  displayedMessages.forEach(({ message, index }, displayedIndex) => {
    if (hiddenMessageCount > 0 && displayedIndex === HEAD_MESSAGE_COUNT) {
      messageRows.push(
        <div
          key="hidden-messages"
          className="my-6 flex items-center justify-center gap-2 text-xs text-text-secondary"
        >
          <span>{intl.formatMessage(i18n.hiddenMessages, { count: hiddenMessageCount })}</span>
          <button
            type="button"
            className="underline underline-offset-2 hover:text-text-primary"
            onClick={() => setShowAllMessages(true)}
          >
            {intl.formatMessage(i18n.showAllMessages)}
          </button>
        </div>
      );
    }

    const isUser = isUserMessage(message);
    const messageIdentifier = message.id ?? `msg-${index}-${message.created}`;
    const messageKey = getSystemNotification(message)
      ? `notification-${messageIdentifier}`
      : messageIdentifier;
    const rowContext = rowContexts[index];
    const currentModel = inferenceModel(message);
    const previousModel = rowContext.previousModel;
    const modelChangeMessage =
      currentModel && previousModel && isModelChange(previousModel, currentModel)
        ? intl.formatMessage(i18n.modelChanged, {
            previousModel: modelLabel(previousModel),
            currentModel: modelLabel(currentModel),
          })
        : null;
    const toolNotifications = rowContext.toolStates.map((toolState) =>
      toolCallNotifications.get(toolState.requestId)
    );

    messageRows.push(
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
  });

  return <>{messageRows}</>;
}
