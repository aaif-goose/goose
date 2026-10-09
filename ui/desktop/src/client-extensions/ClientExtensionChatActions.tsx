import { useCallback, useMemo, useRef, useState } from 'react';
import { Button } from '../components/ui/button';
import { Tooltip, TooltipContent, TooltipTrigger } from '../components/ui/Tooltip';
import { cn } from '../utils';
import { toastService } from '../toasts';
import { notifyExtensionActivate, routeExtensionToHostMessage } from './extensionHostBridge';
import { useClientExtensions, useExtensionHostContext } from './ClientExtensionsContext';
import { parseExtensionToHostMessage } from './messages';
import { PLUGIN_FRAME_SANDBOX } from './sandbox';
import { useExtensionHostSession } from './useExtensionHostSession';
import type { HostToExtensionMessage, RegisteredChatAction } from './types';
import { useWindowMessage } from '../hooks/useWindowMessage';
import { defineMessages, useIntl } from '../i18n';

const i18n = defineMessages({
  loadFailed: {
    id: 'clientExtensionChatActions.loadFailed',
    defaultMessage: 'Plugin failed to load',
  },
  actionFailed: {
    id: 'clientExtensionChatActions.actionFailed',
    defaultMessage: 'Plugin action failed',
  },
});

function ClientExtensionActionButton({
  action,
  hostContext,
  onSetInput,
}: {
  action: RegisteredChatAction;
  hostContext: ReturnType<typeof useExtensionHostContext>;
  onSetInput?: (text: string) => void;
}) {
  const intl = useIntl();
  const { extensions, getExtensionFrameDocument } = useClientExtensions();
  const extension = useMemo(
    () => extensions.find((entry) => entry.id === action.extensionId),
    [extensions, action.extensionId]
  );
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const pendingActionRef = useRef(false);
  const [html, setHtml] = useState<string | null>(null);
  const [activating, setActivating] = useState(false);

  const postToExtension = useCallback((payload: unknown) => {
    iframeRef.current?.contentWindow?.postMessage(payload, '*');
  }, []);
  const hostSessionRef = useExtensionHostSession(extension, postToExtension);

  const sendAction = useCallback(() => {
    const message: HostToExtensionMessage = {
      type: 'grc/action',
      actionId: action.id,
      context: hostContext,
    };
    postToExtension(message);
  }, [action.id, hostContext, postToExtension]);

  const handleLoad = useCallback(() => {
    const message: HostToExtensionMessage = {
      type: 'grc/activate',
      viewId: action.id,
      viewKind: 'chatAction',
      context: hostContext,
    };
    notifyExtensionActivate(iframeRef.current, message, hostSessionRef.current);

    if (pendingActionRef.current) {
      pendingActionRef.current = false;
      sendAction();
    }
    setActivating(false);
  }, [action.id, hostContext, hostSessionRef, sendAction]);

  const handleExtensionMessage = useCallback(
    async (event: MessageEvent) => {
      const hostSession = hostSessionRef.current;
      if (event.source !== iframeRef.current?.contentWindow || !hostSession) {
        return;
      }

      const message = parseExtensionToHostMessage(event.data);
      if (!message) {
        return;
      }

      const handled = await routeExtensionToHostMessage(hostSession, message, action.label);
      if (!handled && message.type === 'grc/chat/setInput') {
        onSetInput?.(message.text);
      }
    },
    [action.label, hostSessionRef, onSetInput]
  );

  useWindowMessage(handleExtensionMessage);

  const onClick = useCallback(async () => {
    if (activating) {
      return;
    }

    if (html) {
      sendAction();
      return;
    }

    setActivating(true);
    pendingActionRef.current = true;
    try {
      const content = await getExtensionFrameDocument(action.extensionId);
      if (!content) {
        toastService.error({ title: action.label, msg: intl.formatMessage(i18n.loadFailed) });
        pendingActionRef.current = false;
        setActivating(false);
        return;
      }
      setHtml(content);
    } catch (error) {
      console.warn('[client-extensions] Action failed:', error);
      toastService.error({ title: action.label, msg: intl.formatMessage(i18n.actionFailed) });
      pendingActionRef.current = false;
      setActivating(false);
    }
  }, [
    action.extensionId,
    action.label,
    activating,
    getExtensionFrameDocument,
    html,
    intl,
    sendAction,
  ]);

  return (
    <>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            shape="round"
            disabled={activating}
            onClick={() => void onClick()}
            className={cn(
              'text-text-primary/70 hover:text-text-primary transition-colors',
              activating && 'opacity-50'
            )}
          >
            <span className="text-xs font-medium px-0.5">{action.label}</span>
          </Button>
        </TooltipTrigger>
        <TooltipContent>{action.label}</TooltipContent>
      </Tooltip>
      {html && (
        <iframe
          ref={iframeRef}
          title={`${action.extensionId} runtime`}
          sandbox={PLUGIN_FRAME_SANDBOX}
          srcDoc={html}
          onLoad={handleLoad}
          aria-hidden="true"
          style={{ position: 'absolute', width: 0, height: 0, border: 0, visibility: 'hidden' }}
        />
      )}
    </>
  );
}

export function ClientExtensionChatActions({
  sessionId,
  onSetInput,
}: {
  sessionId: string | null;
  onSetInput?: (text: string) => void;
}) {
  const hostContext = useExtensionHostContext(sessionId);
  const { getChatActions, loading, registryVersion } = useClientExtensions();
  const actions = useMemo(() => getChatActions(hostContext), [getChatActions, hostContext]);

  if (loading || actions.length === 0) {
    return null;
  }

  return (
    <>
      {actions.map((action) => (
        <ClientExtensionActionButton
          key={`${registryVersion}:${action.extensionId}:${action.id}`}
          action={action}
          hostContext={hostContext}
          onSetInput={onSetInput}
        />
      ))}
    </>
  );
}
