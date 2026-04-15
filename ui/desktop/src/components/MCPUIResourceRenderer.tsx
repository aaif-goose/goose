import { AppEvents } from '../constants/events';
import {
  UIResourceRenderer,
  UIActionResultIntent,
  UIActionResultLink,
  UIActionResultNotification,
  UIActionResultPrompt,
  UIActionResultToolCall,
  UIActionResult,
} from '@mcp-ui/client';
import { useState, useEffect } from 'react';
import { toast } from 'react-toastify';
import { EmbeddedResource } from '../api';
import { useTheme } from '../contexts/ThemeContext';
import { errorMessage } from '../utils/conversionUtils';
import { isProtocolSafe, getProtocol } from '../utils/urlSecurity';
import { defineMessages, useIntl } from '../i18n';

const i18n = defineMessages({
  toastTitle: {
    id: 'mcpUIResourceRenderer.toastTitle',
    defaultMessage: 'MCP-UI {messageType} message',
  },
  toastMessageReceived: {
    id: 'mcpUIResourceRenderer.toastMessageReceived',
    defaultMessage: 'Message received for {message}.',
  },
  toastUnsupported: {
    id: 'mcpUIResourceRenderer.toastUnsupported',
    defaultMessage:
      "Message received for {message}. {messageType} messages aren't supported yet, refer to console for more details.",
  },
  openExternalLinkTitle: {
    id: 'mcpUIResourceRenderer.openExternalLinkTitle',
    defaultMessage: 'Open External Link',
  },
  openProtocolLink: {
    id: 'mcpUIResourceRenderer.openProtocolLink',
    defaultMessage: 'Open {protocol} link?',
  },
  openLinkDetail: {
    id: 'mcpUIResourceRenderer.openLinkDetail',
    defaultMessage: 'This will open: {url}',
  },
  cancelButton: {
    id: 'mcpUIResourceRenderer.cancelButton',
    defaultMessage: 'Cancel',
  },
  openButton: {
    id: 'mcpUIResourceRenderer.openButton',
    defaultMessage: 'Open',
  },
});

interface MCPUIResourceRendererProps {
  content: EmbeddedResource & { type: 'resource' };
  appendPromptToChat?: (value: string) => void;
}

// More specific result types using discriminated unions
type UIActionHandlerSuccess<T = unknown> = {
  status: 'success';
  data?: T;
  message?: string;
};

type UIActionHandlerError = {
  status: 'error';
  error: {
    code: UIActionErrorCode;
    message: string;
    details?: unknown;
  };
};

type UIActionHandlerPending = {
  status: 'pending';
  message: string;
};

type UIActionHandlerResult<T = unknown> =
  | UIActionHandlerSuccess<T>
  | UIActionHandlerError
  | UIActionHandlerPending;

// Strongly typed error codes
enum UIActionErrorCode {
  UNSUPPORTED_ACTION = 'UNSUPPORTED_ACTION',
  UNKNOWN_ACTION = 'UNKNOWN_ACTION',
  TOOL_NOT_FOUND = 'TOOL_NOT_FOUND',
  TOOL_EXECUTION_FAILED = 'TOOL_EXECUTION_FAILED',
  NAVIGATION_FAILED = 'NAVIGATION_FAILED',
  PROMPT_FAILED = 'PROMPT_FAILED',
  INTENT_FAILED = 'INTENT_FAILED',
  INVALID_PARAMS = 'INVALID_PARAMS',
  NETWORK_ERROR = 'NETWORK_ERROR',
  TIMEOUT = 'TIMEOUT',
}

// toast component
const ToastComponent = ({
  messageType,
  message,
  isImplemented = true,
}: {
  messageType: string;
  message?: string;
  isImplemented?: boolean;
}) => {
  const intl = useIntl();
  const title = intl.formatMessage(i18n.toastTitle, { messageType });

  return (
    <div className="flex flex-col gap-0 py-2 pr-4">
      <p className="font-bold">{title}</p>
      {isImplemented ? (
        <p>
          {intl.formatMessage(i18n.toastMessageReceived, {
            message: <span className="font-bold">{message}</span>,
          })}
        </p>
      ) : (
        <p>
          {intl.formatMessage(i18n.toastUnsupported, {
            message: <span className="font-bold">{message}</span>,
            messageType: messageType.charAt(0).toUpperCase() + messageType.slice(1),
          })}
        </p>
      )}
    </div>
  );
};

export default function MCPUIResourceRenderer({
  content,
  appendPromptToChat,
}: MCPUIResourceRendererProps) {
  const intl = useIntl();
  const { resolvedTheme } = useTheme();
  const [proxyUrl, setProxyUrl] = useState<string | undefined>(undefined);

  useEffect(() => {
    const fetchProxyUrl = async () => {
      try {
        const gooseApiHost = await window.electron.getGoosedHostPort();
        const secretKey = await window.electron.getSecretKey();
        if (gooseApiHost && secretKey) {
          setProxyUrl(`${gooseApiHost}/mcp-ui-proxy?secret=${encodeURIComponent(secretKey)}`);
        } else {
          console.error('Failed to get goosed host/port or secret key');
        }
      } catch (error) {
        console.error('Error fetching MCP-UI Proxy URL:', error);
      }
    };

    fetchProxyUrl().catch(console.error);
  }, []);

  const handleUIAction = async (actionEvent: UIActionResult): Promise<UIActionHandlerResult> => {
    // result to pass back to the MCP-UI
    let result: UIActionHandlerResult;

    const handleToolAction = async (
      actionEvent: UIActionResultToolCall
    ): Promise<UIActionHandlerResult> => {
      const { toolName, params } = actionEvent.payload;
      toast.info(<ToastComponent messageType="tool" message={toolName} isImplemented={false} />, {
        theme: resolvedTheme,
      });
      return {
        status: 'error' as const,
        error: {
          code: UIActionErrorCode.UNSUPPORTED_ACTION,
          message: 'Tool calls are not yet implemented',
          details: { toolName, params },
        },
      };
    };

    const handlePromptAction = async (
      actionEvent: UIActionResultPrompt
    ): Promise<UIActionHandlerResult> => {
      const { prompt } = actionEvent.payload;

      if (appendPromptToChat) {
        try {
          appendPromptToChat(prompt);
          window.dispatchEvent(new CustomEvent(AppEvents.SCROLL_CHAT_TO_BOTTOM));
          return {
            status: 'success' as const,
            message: 'Prompt sent to chat successfully',
          };
        } catch (error) {
          return {
            status: 'error' as const,
            error: {
              code: UIActionErrorCode.PROMPT_FAILED,
              message: 'Failed to send prompt to chat',
              details: errorMessage(error),
            },
          };
        }
      }

      return {
        status: 'error' as const,
        error: {
          code: UIActionErrorCode.UNSUPPORTED_ACTION,
          message: 'Prompt handling is not implemented - append prop is required',
          details: { prompt },
        },
      };
    };

    const handleLinkAction = async (
      actionEvent: UIActionResultLink
    ): Promise<UIActionHandlerResult> => {
      const { url } = actionEvent.payload;

      try {
        // Safe protocols open directly, unknown protocols require user confirmation
        // Dangerous protocols are blocked by main.ts in the open-external handler
        if (isProtocolSafe(url)) {
          await window.electron.openExternal(url);
          return {
            status: 'success' as const,
            message: `Opened ${url} in default application`,
          };
        }

        // Unknown protocols require user confirmation
        const protocol = getProtocol(url);
        if (!protocol) {
          return {
            status: 'error' as const,
            error: {
              code: UIActionErrorCode.INVALID_PARAMS,
              message: `Invalid URL format: ${url}`,
              details: { url },
            },
          };
        }

        const result = await window.electron.showMessageBox({
          type: 'question',
          buttons: [
            intl.formatMessage(i18n.cancelButton),
            intl.formatMessage(i18n.openButton),
          ],
          defaultId: 0,
          title: intl.formatMessage(i18n.openExternalLinkTitle),
          message: intl.formatMessage(i18n.openProtocolLink, { protocol }),
          detail: intl.formatMessage(i18n.openLinkDetail, { url }),
        });

        if (result.response !== 1) {
          return {
            status: 'error' as const,
            error: {
              code: UIActionErrorCode.NAVIGATION_FAILED,
              message: 'User cancelled',
              details: { url },
            },
          };
        }

        await window.electron.openExternal(url);
        return {
          status: 'success' as const,
          message: `Opened ${url} in default application`,
        };
      } catch (error) {
        return {
          status: 'error' as const,
          error: {
            code: UIActionErrorCode.NAVIGATION_FAILED,
            message: `Failed to open URL: ${url}`,
            details: errorMessage(error),
          },
        };
      }
    };

    const handleNotifyAction = async (
      actionEvent: UIActionResultNotification
    ): Promise<UIActionHandlerResult> => {
      const { message } = actionEvent.payload;

      toast.info(<ToastComponent messageType="notify" message={message} isImplemented={true} />, {
        theme: resolvedTheme,
      });
      return {
        status: 'success' as const,
        data: {
          displayedAt: new Date().toISOString(),
          message: 'Notification displayed',
          details: actionEvent.payload,
        },
      };
    };

    const handleIntentAction = async (
      actionEvent: UIActionResultIntent
    ): Promise<UIActionHandlerResult> => {
      toast.info(
        <ToastComponent
          messageType="intent"
          message={actionEvent.payload.intent}
          isImplemented={false}
        />,
        {
          theme: resolvedTheme,
        }
      );
      return {
        status: 'error' as const,
        error: {
          code: UIActionErrorCode.UNSUPPORTED_ACTION,
          message: 'Intent handling is not yet implemented',
          details: actionEvent.payload,
        },
      };
    };

    try {
      switch (actionEvent.type) {
        case 'tool':
          result = await handleToolAction(actionEvent);
          break;

        case 'prompt':
          result = await handlePromptAction(actionEvent);
          break;

        case 'link':
          result = await handleLinkAction(actionEvent);
          break;

        case 'notify':
          result = await handleNotifyAction(actionEvent);
          break;

        case 'intent':
          result = await handleIntentAction(actionEvent);
          break;

        default: {
          const _exhaustiveCheck: never = actionEvent;
          console.error('Unhandled MCP-UI action type:', _exhaustiveCheck);
          result = {
            status: 'error',
            error: {
              code: UIActionErrorCode.UNKNOWN_ACTION,
              message: `Unknown action type`,
              details: actionEvent,
            },
          };
        }
      }
    } catch (error) {
      console.error('Unexpected error handling MCP-UI action:', error);
      result = {
        status: 'error',
        error: {
          code: UIActionErrorCode.UNKNOWN_ACTION,
          message: 'An unexpected error occurred',
          details: error instanceof Error ? error.stack : error,
        },
      };
    }

    return result;
  };

  return (
    <div className="mt-3 p-4 border border-border-primary rounded-lg bg-background-secondary">
      <div className="overflow-hidden rounded-sm">
        <UIResourceRenderer
          resource={content.resource}
          onUIAction={handleUIAction}
          supportedContentTypes={['rawHtml', 'externalUrl']} // Goose does not support remoteDom content
          htmlProps={{
            autoResizeIframe: {
              height: true,
              width: false, // set to false to allow for responsive design
            },
            iframeRenderData: {
              // iframeRenderData allows us to pass data down to MCP-UIs
              // MCP-UIs might find stuff like host and theme for conditional rendering
              // usage of this is experimental, leaving in place for demos
              host: 'goose',
              theme: resolvedTheme,
            },
            proxy: proxyUrl, // refer to https://mcpui.dev/guide/client/using-a-proxy
          }}
        />
      </div>
    </div>
  );
}
