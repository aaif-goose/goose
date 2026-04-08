import { useMemo, useRef } from 'react';
import ImagePreview from './ImagePreview';
import { formatMessageTimestamp } from '../utils/timeUtils';
import MarkdownContent from './MarkdownContent';
import ThinkingContent from './ThinkingContent';
import ToolCallWithResponse from './ToolCallWithResponse';
import {
  getTextAndImageContent,
  getThinkingContent,
  getToolRequests,
  getToolResponses,
  getToolConfirmationContent,
  getElicitationContent,
  getPendingToolConfirmationIds,
  getAnyToolConfirmationData,
  ToolConfirmationData,
  NotificationEvent,
} from '../types/message';
import { Message } from '../api';
import ToolCallConfirmation from './ToolCallConfirmation';
import ElicitationRequest from './ElicitationRequest';
import MessageCopyLink from './MessageCopyLink';
import { cn } from '../utils';
import { identifyConsecutiveToolCalls, shouldHideTimestamp } from '../utils/toolCallChaining';

interface GooseMessageProps {
  sessionId: string;
  message: Message;
  messages: Message[];
  metadata?: string[];
  toolCallNotifications: Map<string, NotificationEvent[]>;
  append: (value: string) => void;
  isStreaming: boolean;
  submitElicitationResponse?: (
    elicitationId: string,
    userData: Record<string, unknown>
  ) => Promise<void>;
}

export default function GooseMessage({
  sessionId,
  message,
  messages,
  toolCallNotifications,
  append,
  isStreaming,
  submitElicitationResponse,
}: GooseMessageProps) {
  const contentRef = useRef<HTMLDivElement | null>(null);

  const { textContent: displayText, imagePaths } = getTextAndImageContent(message);
  const thinkingContent = getThinkingContent(message);

  const timestamp = useMemo(() => formatMessageTimestamp(message.created), [message.created]);
  const toolRequests = getToolRequests(message);
  const messageIndex = messages.findIndex((msg) => msg.id === message.id);
  const toolConfirmationContent = getToolConfirmationContent(message);
  const elicitationContent = getElicitationContent(message);

  const findConfirmationForToolAcrossMessages = (
    toolRequestId: string
  ): ToolConfirmationData | undefined => {
    for (const msg of messages) {
      const confirmationData = getAnyToolConfirmationData(msg);
      if (confirmationData && confirmationData.id === toolRequestId) {
        return confirmationData;
      }
    }
    return undefined;
  };
  const toolCallChains = useMemo(() => identifyConsecutiveToolCalls(messages), [messages]);
  const hideTimestamp = useMemo(
    () => shouldHideTimestamp(messageIndex, toolCallChains),
    [messageIndex, toolCallChains]
  );
  const hasToolConfirmation = toolConfirmationContent !== undefined;
  const hasElicitation = elicitationContent !== undefined;

  const toolConfirmationShownInline = useMemo(() => {
    if (!toolConfirmationContent) return false;
    const confirmationData = getAnyToolConfirmationData(message);
    if (!confirmationData) return false;

    for (const msg of messages) {
      const requests = getToolRequests(msg);
      if (requests.some((req) => req.id === confirmationData.id)) {
        return true;
      }
    }
    return false;
  }, [toolConfirmationContent, message, messages]);

  const toolResponsesMap = useMemo(() => {
    const responseMap = new Map();

    if (messageIndex !== undefined && messageIndex >= 0) {
      for (let i = messageIndex + 1; i < messages.length; i++) {
        const responses = getToolResponses(messages[i]);

        for (const response of responses) {
          const matchingRequest = toolRequests.find((req) => req.id === response.id);
          if (matchingRequest) {
            responseMap.set(response.id, response);
          }
        }
      }
    }

    return responseMap;
  }, [messages, messageIndex, toolRequests]);

  const pendingConfirmationIds = getPendingToolConfirmationIds(messages);

  return (
    <div className="goose-message flex w-[90%] justify-start min-w-0">
      <div className="flex flex-col w-full min-w-0">
        {thinkingContent && (
          <ThinkingContent
            content={thinkingContent}
            isExpanded={isStreaming && !displayText.trim() && imagePaths.length === 0 && toolRequests.length === 0}
          />
        )}

        {(displayText.trim() || imagePaths.length > 0) && (
          <div className="flex flex-col group">
            {displayText.trim() && (
              <div ref={contentRef} className="w-full">
                <MarkdownContent content={displayText} />
              </div>
            )}

            {imagePaths.length > 0 && (
              <div className="mt-4">
                {imagePaths.map((imagePath, index) => (
                  <ImagePreview key={index} src={imagePath} />
                ))}
              </div>
            )}

            {toolRequests.length === 0 && (
              <div className="relative flex justify-start">
                {!isStreaming && (
                  <div className="text-xs font-mono text-text-secondary pt-1 transition-all duration-200 group-hover:-translate-y-4 group-hover:opacity-0">
                    {timestamp}
                  </div>
                )}
                {message.content.every((content) => content.type === 'text') && !isStreaming && (
                  <div className="absolute left-0 pt-1">
                    <MessageCopyLink text={displayText} contentRef={contentRef} />
                  </div>
                )}
              </div>
            )}
          </div>
        )}

        {toolRequests.length > 0 && (
          <div className={cn(displayText && 'mt-2')}>
            <div className="relative flex flex-col w-full">
              <div className="flex flex-col gap-3">
                {toolRequests.map((toolRequest) => {
                  const hasResponse = toolResponsesMap.has(toolRequest.id);
                  const isPending = pendingConfirmationIds.has(toolRequest.id);
                  const confirmationContent = findConfirmationForToolAcrossMessages(toolRequest.id);
                  const isApprovalClicked = confirmationContent && !isPending && hasResponse;
                  return (
                    <div className="goose-message-tool" key={toolRequest.id}>
                      <ToolCallWithResponse
                        sessionId={sessionId}
                        isCancelledMessage={false}
                        toolRequest={toolRequest}
                        toolResponse={toolResponsesMap.get(toolRequest.id)}
                        notifications={toolCallNotifications.get(toolRequest.id)}
                        isStreamingMessage={isStreaming}
                        isPendingApproval={isPending}
                        append={append}
                        confirmationContent={confirmationContent}
                        isApprovalClicked={isApprovalClicked}
                      />
                    </div>
                  );
                })}
              </div>
              <div className="text-xs text-text-secondary transition-all duration-200 group-hover:-translate-y-4 group-hover:opacity-0 pt-1">
                {!isStreaming && !hideTimestamp && timestamp}
              </div>
            </div>
          </div>
        )}

        {hasToolConfirmation && !toolConfirmationShownInline && (
          <ToolCallConfirmation
            sessionId={sessionId}
            isClicked={false}
            actionRequiredContent={toolConfirmationContent}
          />
        )}

        {hasElicitation && submitElicitationResponse && (
          <ElicitationRequest
            isCancelledMessage={false}
            isClicked={false}
            actionRequiredContent={elicitationContent}
            onSubmit={submitElicitationResponse}
          />
        )}
      </div>
    </div>
  );
}
