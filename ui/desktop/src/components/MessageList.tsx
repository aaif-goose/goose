import { Fragment, memo, useEffect, useMemo, useState } from 'react';
import { isEqual } from 'lodash';
import { defineMessages, useIntl } from '../i18n';
import GooseMessage, { type MessagePresentation } from './GooseMessage';
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
  getToolRequests,
  type ImageData,
  type Message,
  type NotificationEvent,
  type SystemNotificationContent,
} from '../types/message';
import { getModelDisplayName } from './settings/models/predefinedModelsUtils';
import {
  type InferenceModel,
  inferenceModel,
  isModelChange,
  type MessageRowContext,
} from './messageRowContext';
import TurnActivityRow from './TurnActivityRow';
import { useSearchVisibility } from './conversation/SearchView';
import type { useTurnActivity } from './useTurnActivity';

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
  presentation: MessagePresentation;
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
  presentation,
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

  if (notification && message.content.every((content) => content.type === 'systemNotification')) {
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
        {notification && renderSystemNotification(notification)}
        {isUser ? (
          !hasOnlyToolResponses && (
            <UserMessage message={message} onMessageUpdate={onMessageUpdate} />
          )
        ) : (
          <GooseMessage
            sessionId={sessionId}
            message={message}
            presentation={presentation}
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
  activity: ReturnType<typeof useTurnActivity>;
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
  activity: { visibleMessages, rowContexts, turns, showThinking },
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
  const isSearching = useSearchVisibility();
  const [showAllMessages, setShowAllMessages] = useState(false);
  const latestTurn = turns.at(-1);
  const latestTurnStart =
    latestTurn && (latestTurn.toolCount > 0 || latestTurn.active)
      ? visibleMessages.findIndex(({ index }) => index === latestTurn.start)
      : visibleMessages.length;
  const tailStart = Math.min(visibleMessages.length - TAIL_MESSAGE_COUNT, latestTurnStart);
  const hiddenMessageCount = showAllMessages ? 0 : Math.max(0, tailStart - HEAD_MESSAGE_COUNT);
  const hasHiddenMessages = hiddenMessageCount > 0;
  const displayedMessages = useMemo(() => {
    if (!hasHiddenMessages) {
      return visibleMessages;
    }

    return [...visibleMessages.slice(0, HEAD_MESSAGE_COUNT), ...visibleMessages.slice(tailStart)];
  }, [hasHiddenMessages, tailStart, visibleMessages]);

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

  const modelLabel = (model: InferenceModel) =>
    model.confirmed
      ? getModelDisplayName(model.id)
      : intl.formatMessage(i18n.requestedModel, { model: getModelDisplayName(model.id) });
  const messageRows: React.ReactNode[] = [];
  let turnIndex = 0;
  const shownSummaries = new Set<number>();

  const renderRow = (
    message: Message,
    index: number,
    key: string,
    turnPresentation?: Exclude<MessagePresentation, { kind: 'standalone' }>
  ) => {
    const isUser = isUserMessage(message);
    const context = rowContexts[index];
    const presentation: MessagePresentation = turnPresentation ?? {
      kind: 'standalone',
      hideTimestamp: context.hideTimestamp,
    };
    const requestIds = new Set(getToolRequests(message).map((request) => request.id));
    const rowContext = {
      ...context,
      isInToolCallChain: presentation.kind === 'standalone' && context.isInToolCallChain,
      toolStates: context.toolStates.filter((state) => requestIds.has(state.requestId)),
    };
    const currentModel = presentation.kind === 'activity' ? null : inferenceModel(message);
    const previousModel = rowContext.previousModel;
    const modelChangeMessage =
      currentModel && previousModel && isModelChange(previousModel, currentModel)
        ? intl.formatMessage(i18n.modelChanged, {
            previousModel: modelLabel(previousModel),
            currentModel: modelLabel(currentModel),
          })
        : null;
    return (
      <MessageRow
        key={key}
        append={append}
        index={presentation.kind === 'activity' ? 0 : index}
        isStreaming={
          !isUser &&
          (presentation.kind === 'turnFooter'
            ? presentation.isTurnActive
            : isStreamingMessage && index === messages.length - 1 && message.role === 'assistant')
        }
        isUser={isUser}
        message={message}
        modelChangeMessage={modelChangeMessage}
        onMessageUpdate={onMessageUpdate}
        rowContext={rowContext}
        sessionId={sessionId}
        submitElicitationResponse={submitElicitationResponse}
        toolNotifications={rowContext.toolStates.map((toolState) =>
          toolCallNotifications.get(toolState.requestId)
        )}
        presentation={presentation}
      />
    );
  };

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

    const messageIdentifier = message.id ?? `msg-${index}-${message.created}`;
    const notification = getSystemNotification(message);
    const notificationOnly =
      notification && message.content.every((content) => content.type === 'systemNotification');
    while (turns[turnIndex].end < index) turnIndex++;
    const turn = turns[turnIndex];
    const hasActivity = turn.steps.length > 0 || turn.toolCount > 0;
    if (
      hasActivity &&
      !notification &&
      message.content.every((content) => content.type === 'toolResponse')
    ) {
      return;
    }
    if (notificationOnly || message.role !== 'assistant' || !hasActivity) {
      messageRows.push(
        renderRow(
          message,
          index,
          notification ? `notification-${messageIdentifier}` : messageIdentifier
        )
      );
      return;
    }

    if (!shownSummaries.has(turn.start)) {
      shownSummaries.add(turn.start);
      const turnStart = messages[turn.start];
      messageRows.push(
        <TurnActivityRow
          key={`turn-${turnStart.id ?? `${turn.start}-${turnStart.created}`}`}
          turn={turn}
          showThinking={showThinking}
          isSearching={isSearching}
          renderSteps={() =>
            turn.steps.map((step) =>
              renderRow(step.message, step.index, `${step.message.id ?? step.index}-${step.key}`, {
                kind: 'activity',
                isTurnActive: turn.active,
              })
            )
          }
        />
      );
    }

    const inline = turn.inline.get(index);
    const previousModel = rowContexts[index].previousModel;
    const currentModel = inferenceModel(message);
    if (inline || (currentModel && previousModel && isModelChange(previousModel, currentModel))) {
      messageRows.push(
        renderRow(inline ?? { ...message, content: [] }, index, messageIdentifier, {
          kind: turn.footerIndex === index ? 'turnFooter' : 'turnInline',
          isTurnActive: turn.active,
        })
      );
    }
  });

  return <>{messageRows}</>;
}
