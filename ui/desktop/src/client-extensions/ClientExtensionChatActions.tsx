import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Button } from '../components/ui/button';
import { Tooltip, TooltipContent, TooltipTrigger } from '../components/ui/Tooltip';
import { cn } from '../utils';
import { toastService } from '../toasts';
import { useClientExtensions, useExtensionHostContext } from './ClientExtensionsContext';
import { parseExtensionToHostMessage } from './messages';
import { PLUGIN_FRAME_SANDBOX } from './sandbox';
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

interface ExtensionRuntime {
  iframe: HTMLIFrameElement;
  ready: boolean;
}

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
  const { getExtensionFrameDocument } = useClientExtensions();
  const runtimeRef = useRef<ExtensionRuntime | null>(null);
  const [activating, setActivating] = useState(false);

  const handleExtensionMessage = useCallback(
    (event: MessageEvent) => {
      const runtime = runtimeRef.current;
      if (!runtime || event.source !== runtime.iframe.contentWindow) {
        return;
      }

      const message = parseExtensionToHostMessage(event.data);
      if (!message) {
        return;
      }

      switch (message.type) {
        case 'grc/ui/showMessage':
          toastService.success({ title: action.label, msg: message.text });
          break;
        case 'grc/chat/setInput':
          onSetInput?.(message.text);
          break;
        default:
          break;
      }
    },
    [action.label, onSetInput]
  );

  useWindowMessage(handleExtensionMessage);

  const ensureRuntime = useCallback(async () => {
    if (runtimeRef.current) {
      return runtimeRef.current;
    }

    const html = await getExtensionFrameDocument(action.extensionId);
    if (!html) {
      return null;
    }

    const iframe = document.createElement('iframe');
    iframe.title = `${action.extensionId} runtime`;
    iframe.setAttribute('sandbox', PLUGIN_FRAME_SANDBOX);
    iframe.setAttribute('aria-hidden', 'true');
    iframe.style.cssText = 'position:absolute;width:0;height:0;border:0;visibility:hidden';
    iframe.srcdoc = html;
    document.body.appendChild(iframe);

    const runtime: ExtensionRuntime = { iframe, ready: false };
    runtimeRef.current = runtime;

    await new Promise<void>((resolve) => {
      const onLoad = () => {
        runtime.ready = true;
        resolve();
      };
      iframe.addEventListener('load', onLoad, { once: true });
    });

    return runtime;
  }, [action.extensionId, getExtensionFrameDocument]);

  useEffect(() => {
    return () => {
      runtimeRef.current?.iframe.remove();
      runtimeRef.current = null;
    };
  }, []);

  const onClick = useCallback(async () => {
    if (activating) {
      return;
    }

    setActivating(true);
    try {
      const runtime = await ensureRuntime();
      if (!runtime?.iframe.contentWindow) {
        toastService.error({
          title: action.label,
          msg: intl.formatMessage(i18n.loadFailed),
        });
        return;
      }

      const message: HostToExtensionMessage = {
        type: 'grc/action',
        actionId: action.id,
        context: hostContext,
      };
      runtime.iframe.contentWindow.postMessage(message, '*');
    } catch (error) {
      console.warn('[client-extensions] Action failed:', error);
      toastService.error({
        title: action.label,
        msg: intl.formatMessage(i18n.actionFailed),
      });
    } finally {
      setActivating(false);
    }
  }, [action.id, action.label, activating, ensureRuntime, hostContext, intl]);

  return (
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
