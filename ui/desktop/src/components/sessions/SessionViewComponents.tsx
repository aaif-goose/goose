import React from 'react';
import { MessageSquare, AlertCircle } from 'lucide-react';
import { defineMessages, useIntl } from '../../i18n';
import { Card } from '../ui/card';
import { Button } from '../ui/button';
import { ScrollArea } from '../ui/scroll-area';
import MarkdownContent from '../MarkdownContent';
import ToolCallWithResponse from '../ToolCallWithResponse';
import ImagePreview from '../ImagePreview';
import {
  getTextAndImageContent,
  getThinkingContent,
  ToolRequestMessageContent,
  ToolResponseMessageContent,
} from '../../types/message';
import { formatMessageTimestamp } from '../../utils/timeUtils';
import { Message } from '../../api';

const i18n = defineMessages({
  errorLoadingDetails: {
    id: 'sessionViewComponents.error.loading',
    defaultMessage: 'Error Loading Session Details',
  },
  tryAgain: {
    id: 'sessionViewComponents.error.tryAgain',
    defaultMessage: 'Try Again',
  },
  noMessages: {
    id: 'sessionViewComponents.empty.title',
    defaultMessage: 'No messages found',
  },
  noMessagesDesc: {
    id: 'sessionViewComponents.empty.description',
    defaultMessage: "This session doesn't contain any messages",
  },
  you: {
    id: 'sessionViewComponents.role.user',
    defaultMessage: 'You',
  },
  goose: {
    id: 'sessionViewComponents.role.assistant',
    defaultMessage: 'Goose',
  },
});

/**
 * Get tool responses map from messages
 */
export const getToolResponsesMap = (
  messages: Message[],
  messageIndex: number,
  toolRequests: ToolRequestMessageContent[]
) => {
  const responseMap = new Map();

  if (messageIndex >= 0) {
    for (let i = messageIndex + 1; i < messages.length; i++) {
      const responses = messages[i].content
        .filter((c) => c.type === 'toolResponse')
        .map((c) => c as ToolResponseMessageContent);

      for (const response of responses) {
        const matchingRequest = toolRequests.find((req) => req.id === response.id);
        if (matchingRequest) {
          responseMap.set(response.id, response);
        }
      }
    }
  }

  return responseMap;
};

interface SessionMessagesProps {
  messages: Message[];
  isLoading: boolean;
  error: string | null;
  onRetry: () => void;
}

/**
 * Common component for displaying session messages
 */
export const SessionMessages: React.FC<SessionMessagesProps> = ({
  messages,
  isLoading,
  error,
  onRetry,
}) => {
  const intl = useIntl();
  return (
    <ScrollArea className="h-full w-full">
      <div className="p-4">
        <div className="flex flex-col space-y-4">
          <div className="space-y-4 mb-6">
            {isLoading ? (
              <div className="flex justify-center items-center py-12">
                <div className="animate-spin rounded-full h-8 w-8 border-t-2 border-b-2"></div>
              </div>
            ) : error ? (
              <div className="flex flex-col items-center justify-center py-8 text-text-secondary">
                <div className="text-red-500 mb-4">
                  <AlertCircle size={32} />
                </div>
                <p className="text-md mb-2">{intl.formatMessage(i18n.errorLoadingDetails)}</p>
                <p className="text-sm text-center mb-4">{error}</p>
                <Button onClick={onRetry} variant="default">
                  {intl.formatMessage(i18n.tryAgain)}
                </Button>
              </div>
            ) : messages?.length > 0 ? (
              messages
                .map((message, index) => {
                  const { textContent, imagePaths } = getTextAndImageContent(message);
                  const thinkingContent = getThinkingContent(message);

                  // Get tool requests from the message
                  const toolRequests = message.content
                    .filter((c) => c.type === 'toolRequest')
                    .map((c) => c as ToolRequestMessageContent);

                  // Get tool responses map using the helper function
                  const toolResponsesMap = getToolResponsesMap(messages, index, toolRequests);

                  // Skip pure tool response messages for cleaner display
                  const isOnlyToolResponse =
                    message.content.length > 0 &&
                    message.content.every((c) => c.type === 'toolResponse');

                  if (message.role === 'user' && isOnlyToolResponse) {
                    return null;
                  }

                  return (
                    <Card
                      key={index}
                      className={`p-4 ${
                        message.role === 'user'
                          ? 'bg-bgSecondary border border-border-primary'
                          : 'bg-background-secondary'
                      }`}
                    >
                      <div className="flex justify-between items-center mb-2">
                        <span className="font-medium text-text-primary">
                          {message.role === 'user' ? intl.formatMessage(i18n.you) : intl.formatMessage(i18n.goose)}
                        </span>
                        <span className="text-xs text-text-secondary">
                          {formatMessageTimestamp(message.created)}
                        </span>
                      </div>

                      <div className="flex flex-col w-full">
                        {/* Thinking content */}
                        {thinkingContent && (
                          <div className="mb-2 text-sm text-gray-400 italic">
                            <MarkdownContent content={thinkingContent} />
                          </div>
                        )}

                        {/* Text content */}
                        {textContent && (
                          <div
                            className={`${toolRequests.length > 0 || imagePaths.length > 0 ? 'mb-4' : ''}`}
                          >
                            <MarkdownContent content={textContent} />
                          </div>
                        )}

                        {imagePaths.length > 0 && (
                          <div className="flex flex-wrap gap-2 mt-2 mb-2">
                            {imagePaths.map((imagePath, imageIndex) => (
                              <ImagePreview key={imageIndex} src={imagePath} />
                            ))}
                          </div>
                        )}

                        {/* Tool requests and responses */}
                        {toolRequests.length > 0 && (
                          <div className="goose-message-tool bg-background-primary border border-border-primary dark:border-gray-700 rounded-b-2xl px-4 pt-4 pb-2 mt-1">
                            {toolRequests.map((toolRequest) => (
                              <ToolCallWithResponse
                                // In the session history page, if no tool response found for given request, it means the tool call
                                // is broken or cancelled.
                                isCancelledMessage={
                                  toolResponsesMap.get(toolRequest.id) == undefined
                                }
                                isPendingApproval={false}
                                key={toolRequest.id}
                                toolRequest={toolRequest}
                                toolResponse={toolResponsesMap.get(toolRequest.id)}
                              />
                            ))}
                          </div>
                        )}
                      </div>
                    </Card>
                  );
                })
                .filter(Boolean) // Filter out null entries
            ) : (
              <div className="flex flex-col items-center justify-center py-8 text-text-secondary">
                <MessageSquare className="w-12 h-12 mb-4" />
                <p className="text-lg mb-2">{intl.formatMessage(i18n.noMessages)}</p>
                <p className="text-sm">{intl.formatMessage(i18n.noMessagesDesc)}</p>
              </div>
            )}
          </div>
        </div>
      </div>
    </ScrollArea>
  );
};
