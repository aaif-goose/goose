import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useLocation } from 'react-router';
import type { Message } from '../types/message';
import { notifyExtensionActivate, routeExtensionToHostMessage } from './extensionHostBridge';
import { useClientExtensions } from './ClientExtensionsContext';
import { parseExtensionToHostMessage } from './messages';
import { PLUGIN_FRAME_SANDBOX } from './sandbox';
import { useExtensionHostSession } from './useExtensionHostSession';
import { buildMessageExtensionContext, extractCodeBlocks } from './messageContext';
import { useWindowMessage } from '../hooks/useWindowMessage';
import type {
  HostToExtensionMessage,
  MessageRenderPayload,
  RegisteredContentSuffix,
} from './types';

function ClientExtensionRenderSlot({
  extensionId,
  slotId,
  slotKind,
  context,
  payload,
}: {
  extensionId: string;
  slotId: string;
  slotKind: 'contentSuffix' | 'customRender';
  context: ReturnType<typeof buildMessageExtensionContext>;
  payload: MessageRenderPayload;
}) {
  const { extensions, getExtensionFrameDocument } = useClientExtensions();
  const extension = useMemo(
    () => extensions.find((entry) => entry.id === extensionId),
    [extensions, extensionId]
  );
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const [height, setHeight] = useState<number | null>(null);
  const [failed, setFailed] = useState(false);

  const postToExtension = useCallback((message: unknown) => {
    iframeRef.current?.contentWindow?.postMessage(message, '*');
  }, []);
  const hostSessionRef = useExtensionHostSession(extension, postToExtension);

  const handleExtensionMessage = useCallback(
    async (event: MessageEvent) => {
      const iframe = iframeRef.current;
      const hostSession = hostSessionRef.current;
      if (!iframe || event.source !== iframe.contentWindow || !hostSession) {
        return;
      }

      const message = parseExtensionToHostMessage(event.data);
      if (!message) {
        return;
      }

      const handled = await routeExtensionToHostMessage(hostSession, message, slotId);
      if (!handled && message.type === 'grc/resize') {
        setHeight(Math.max(0, Math.min(message.height, 480)));
      }
    },
    [hostSessionRef, slotId]
  );

  useWindowMessage(handleExtensionMessage);

  useEffect(() => {
    let cancelled = false;

    void (async () => {
      const html = await getExtensionFrameDocument(extensionId);
      if (cancelled) {
        return;
      }

      if (!html) {
        setFailed(true);
        return;
      }

      const iframe = iframeRef.current;
      if (!iframe) {
        return;
      }

      const onLoad = () => {
        const message: HostToExtensionMessage = {
          type: 'grc/render',
          slotId,
          slotKind,
          context,
          payload,
        };
        notifyExtensionActivate(iframe, message, hostSessionRef.current);
      };

      iframe.addEventListener('load', onLoad, { once: true });
      iframe.srcdoc = html;
    })();

    return () => {
      cancelled = true;
    };
  }, [context, extensionId, getExtensionFrameDocument, hostSessionRef, payload, slotId, slotKind]);

  if (failed) {
    return null;
  }

  return (
    <iframe
      ref={iframeRef}
      title={`${extensionId}:${slotId}`}
      sandbox={PLUGIN_FRAME_SANDBOX}
      className="w-full border-0"
      style={{ height: height ?? 24, minHeight: 24 }}
    />
  );
}

export function ClientExtensionMessageDecorations({
  sessionId,
  message,
  displayText,
  imageCount,
}: {
  sessionId: string;
  message: Message;
  displayText: string;
  imageCount: number;
}) {
  const location = useLocation();
  const { getContentSuffixes, getCustomRender, loading, registryVersion } = useClientExtensions();

  const messageContext = useMemo(
    () =>
      buildMessageExtensionContext(sessionId, location.pathname, message, displayText, imageCount),
    [sessionId, location.pathname, message, displayText, imageCount]
  );

  const codeBlocks = useMemo(() => extractCodeBlocks(displayText), [displayText]);

  const suffixes = useMemo(
    () => (loading ? [] : getContentSuffixes(messageContext)),
    [getContentSuffixes, loading, messageContext]
  );

  const customRender = useMemo(
    () => (loading ? null : getCustomRender(messageContext, codeBlocks)),
    [getCustomRender, loading, messageContext, codeBlocks]
  );

  const basePayload = useMemo(
    (): MessageRenderPayload => ({
      textPreview: displayText.slice(0, 2000),
      codeBlocks,
    }),
    [displayText, codeBlocks]
  );

  if (suffixes.length === 0 && !customRender) {
    return null;
  }

  return (
    <div className="mt-2 flex flex-col gap-2 w-full min-w-0">
      {suffixes.map((suffix: RegisteredContentSuffix) => (
        <ClientExtensionRenderSlot
          key={`${registryVersion}:${suffix.extensionId}:${suffix.id}`}
          extensionId={suffix.extensionId}
          slotId={suffix.id}
          slotKind="contentSuffix"
          context={messageContext}
          payload={basePayload}
        />
      ))}
      {customRender && (
        <ClientExtensionRenderSlot
          key={`${registryVersion}:${customRender.extensionId}:${customRender.id}`}
          extensionId={customRender.extensionId}
          slotId={customRender.id}
          slotKind="customRender"
          context={messageContext}
          payload={{
            ...basePayload,
            matchedLanguage: customRender.match.language,
          }}
        />
      )}
    </div>
  );
}
