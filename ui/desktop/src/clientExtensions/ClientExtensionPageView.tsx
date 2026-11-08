import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from 'react';
import { useLocation } from 'react-router';
import { ArrowLeft } from 'lucide-react';
import { notifyExtensionActivate, routeExtensionToHostMessage } from './extensionHostBridge';
import { parseExtensionToHostMessage } from './messages';
import { PLUGIN_FRAME_SANDBOX } from './sandbox';
import { useClientExtensions, useExtensionHostContext } from './ClientExtensionsContext';
import { useExtensionHostSession } from './useExtensionHostSession';
import { useWindowMessage } from '../hooks/useWindowMessage';
import { parseClientExtensionViewPath } from './routes';
import type { HostToExtensionMessage } from './types';
import { useNavigationSessions } from '../hooks/useNavigationSessions';
import {
  getNativePluginPage,
  getNativePluginsVersion,
  hasNativePluginFailed,
  subscribeNativePlugins,
} from './nativePluginStore';
import { PluginErrorBoundary } from './PluginErrorBoundary';
import { Button } from '../components/ui/button';
import { defineMessages, useIntl } from '../i18n';

const i18n = defineMessages({
  invalidRoute: {
    id: 'clientExtensionPage.invalidRoute',
    defaultMessage: 'Invalid plugin route',
  },
  viewNotFound: {
    id: 'clientExtensionPage.viewNotFound',
    defaultMessage: 'Plugin view not found: {extensionId}/{viewId}',
  },
  loadFailed: {
    id: 'clientExtensionPage.loadFailed',
    defaultMessage: 'Failed to load plugin "{extensionId}"',
  },
  loading: {
    id: 'clientExtensionPage.loading',
    defaultMessage: 'Loading plugin…',
  },
  backToChat: {
    id: 'clientExtensionPage.backToChat',
    defaultMessage: 'Back to chat',
  },
  fallbackTitle: {
    id: 'clientExtensionPage.fallbackTitle',
    defaultMessage: 'Plugin',
  },
});

function SandboxedExtensionPage() {
  const intl = useIntl();
  const location = useLocation();
  const { extensions, getExtensionFrameDocument, registryVersion } = useClientExtensions();
  const hostContext = useExtensionHostContext(null);
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const [html, setHtml] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);

  const view = useMemo(() => parseClientExtensionViewPath(location.pathname), [location.pathname]);

  const extension = useMemo(
    () => (view ? extensions.find((entry) => entry.id === view.extensionId) : undefined),
    [extensions, view]
  );

  const rootLink = useMemo(() => {
    if (!view || !extension) {
      return undefined;
    }
    return extension.manifest.contributes?.rootLinks?.find((link) => link.id === view.viewId);
  }, [extension, view]);

  const postToExtension = useCallback((payload: unknown) => {
    iframeRef.current?.contentWindow?.postMessage(payload, '*');
  }, []);

  const hostSessionRef = useExtensionHostSession(extension, postToExtension);

  useEffect(() => {
    if (!view) {
      setLoadError(intl.formatMessage(i18n.invalidRoute));
      setHtml(null);
      return;
    }

    if (!extension || !extension.enabled || !rootLink) {
      setLoadError(
        intl.formatMessage(i18n.viewNotFound, {
          extensionId: view.extensionId,
          viewId: view.viewId,
        })
      );
      setHtml(null);
      return;
    }

    let cancelled = false;
    setLoadError(null);
    setHtml(null);

    void getExtensionFrameDocument(view.extensionId).then((content) => {
      if (cancelled) {
        return;
      }
      if (!content) {
        setLoadError(intl.formatMessage(i18n.loadFailed, { extensionId: view.extensionId }));
        return;
      }
      setHtml(content);
    });

    return () => {
      cancelled = true;
    };
  }, [extension, getExtensionFrameDocument, intl, registryVersion, rootLink, view]);

  const handleExtensionMessage = useCallback(
    async (event: MessageEvent) => {
      const hostSession = hostSessionRef.current;
      if (event.source !== iframeRef.current?.contentWindow || !view || !hostSession) {
        return;
      }
      const message = parseExtensionToHostMessage(event.data);
      if (!message) {
        return;
      }

      await routeExtensionToHostMessage(
        hostSession,
        message,
        rootLink?.label ?? intl.formatMessage(i18n.fallbackTitle)
      );
    },
    [hostSessionRef, intl, rootLink?.label, view]
  );

  useWindowMessage(handleExtensionMessage);

  const notifyActivate = useCallback(() => {
    if (!view) {
      return;
    }

    const message: HostToExtensionMessage = {
      type: 'grc/activate',
      viewId: view.viewId,
      viewKind: 'rootLink',
      context: hostContext,
    };
    notifyExtensionActivate(iframeRef.current, message, hostSessionRef.current);
  }, [hostContext, hostSessionRef, view]);

  if (loadError) {
    return (
      <div className="flex h-full items-center justify-center p-6 text-sm text-text-secondary">
        {loadError}
      </div>
    );
  }

  if (!html || !rootLink || !view) {
    return (
      <div className="flex h-full items-center justify-center p-6 text-sm text-text-secondary">
        {intl.formatMessage(i18n.loading)}
      </div>
    );
  }

  return (
    <ExtensionPageFrame title={rootLink.label}>
      <iframe
        key={`${registryVersion}:${view.extensionId}:${view.viewId}`}
        ref={iframeRef}
        title={rootLink.label}
        sandbox={PLUGIN_FRAME_SANDBOX}
        srcDoc={html}
        onLoad={notifyActivate}
        className="h-full w-full flex-1 border-0 bg-background-primary"
      />
    </ExtensionPageFrame>
  );
}

function ExtensionPageFrame({ title, children }: { title: string; children: ReactNode }) {
  const intl = useIntl();
  const { handleNavClick } = useNavigationSessions();

  return (
    <div className="flex h-full min-h-0 flex-col bg-background-primary">
      <div className="flex items-center gap-2 border-b border-border-primary px-4 py-3">
        <Button
          type="button"
          variant="ghost"
          size="xs"
          onClick={() => handleNavClick('/pair')}
          className="no-drag"
        >
          <ArrowLeft className="h-4 w-4" />
          {intl.formatMessage(i18n.backToChat)}
        </Button>
        <h1 className="text-sm font-medium text-text-primary">{title}</h1>
      </div>
      {children}
    </div>
  );
}

function ExtensionPageMessage({ children }: { children: ReactNode }) {
  return (
    <div className="flex h-full items-center justify-center p-6 text-sm text-text-secondary">
      {children}
    </div>
  );
}

function NativeExtensionPage({ extensionId, viewId }: { extensionId: string; viewId: string }) {
  const intl = useIntl();
  const { extensions } = useClientExtensions();
  useSyncExternalStore(subscribeNativePlugins, getNativePluginsVersion);

  const rootLink = extensions
    .find((entry) => entry.id === extensionId)
    ?.manifest.contributes?.rootLinks?.find((link) => link.id === viewId);
  const Page = getNativePluginPage(extensionId, viewId);
  const failedMessage = (
    <ExtensionPageMessage>
      {intl.formatMessage(i18n.loadFailed, { extensionId })}
    </ExtensionPageMessage>
  );

  if (!rootLink) {
    return (
      <ExtensionPageMessage>
        {intl.formatMessage(i18n.viewNotFound, { extensionId, viewId })}
      </ExtensionPageMessage>
    );
  }

  return (
    <ExtensionPageFrame title={rootLink.label}>
      {Page ? (
        <PluginErrorBoundary resetKey={Page} fallback={failedMessage}>
          <div className="min-h-0 flex-1 overflow-auto">
            <Page />
          </div>
        </PluginErrorBoundary>
      ) : hasNativePluginFailed(extensionId) ? (
        failedMessage
      ) : (
        <ExtensionPageMessage>{intl.formatMessage(i18n.loading)}</ExtensionPageMessage>
      )}
    </ExtensionPageFrame>
  );
}

export default function ClientExtensionPageView() {
  const location = useLocation();
  const { extensions } = useClientExtensions();
  const view = useMemo(() => parseClientExtensionViewPath(location.pathname), [location.pathname]);
  const extension = view ? extensions.find((entry) => entry.id === view.extensionId) : undefined;

  if (view && extension?.enabled && extension.manifest.runtime === 'native') {
    return <NativeExtensionPage extensionId={view.extensionId} viewId={view.viewId} />;
  }

  return <SandboxedExtensionPage />;
}
