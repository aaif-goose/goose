import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { motion } from 'framer-motion';
import { PanelRight, X } from 'lucide-react';
import { toastService } from '../toasts';
import { useClientExtensions, useExtensionHostContext } from './ClientExtensionsContext';
import { parseExtensionToHostMessage } from './messages';
import { PLUGIN_FRAME_SANDBOX } from './sandbox';
import { useWindowMessage } from '../hooks/useWindowMessage';
import type { HostToExtensionMessage, RegisteredSidecar } from './types';
import { NAV_DIMENSIONS } from '../components/Layout/constants';
import { Button } from '../components/ui/button';
import { cn } from '../utils';
import { defineMessages, useIntl } from '../i18n';

const i18n = defineMessages({
  loadFailed: {
    id: 'clientExtensionSidecar.loadFailed',
    defaultMessage: 'Failed to load plugin "{extensionId}"',
  },
  loading: {
    id: 'clientExtensionSidecar.loading',
    defaultMessage: 'Loading…',
  },
  close: {
    id: 'clientExtensionSidecar.close',
    defaultMessage: 'Close sidecar',
  },
});

function sidecarKey(sidecar: RegisteredSidecar): string {
  return `${sidecar.extensionId}:${sidecar.id}`;
}

interface SidecarContextValue {
  sidecars: RegisteredSidecar[];
  activeKey: string | null;
  activeSidecar: RegisteredSidecar | null;
  toggleSidecar: (sidecar: RegisteredSidecar) => void;
  closeSidecar: () => void;
  hostContext: ReturnType<typeof useExtensionHostContext>;
}

const SidecarContext = createContext<SidecarContextValue | null>(null);

function useSidecarContext(): SidecarContextValue {
  const context = useContext(SidecarContext);
  if (!context) {
    throw new Error('Sidecar components must be used within ClientExtensionSidecarProvider');
  }
  return context;
}

export function ClientExtensionSidecarProvider({
  sessionId,
  children,
}: {
  sessionId: string | null;
  children: React.ReactNode;
}) {
  const hostContext = useExtensionHostContext(sessionId);
  const { getSidecars, loading, registryVersion } = useClientExtensions();
  const sidecars = useMemo(
    () => (loading ? [] : getSidecars(hostContext)),
    [getSidecars, hostContext, loading]
  );
  const [activeKey, setActiveKey] = useState<string | null>(null);
  const defaultAppliedRef = useRef(false);

  const activeSidecar = useMemo(
    () => sidecars.find((sidecar) => sidecarKey(sidecar) === activeKey) ?? null,
    [activeKey, sidecars]
  );

  useEffect(() => {
    if (activeKey && !activeSidecar) {
      setActiveKey(null);
    }
  }, [activeKey, activeSidecar]);

  useEffect(() => {
    if (defaultAppliedRef.current || sidecars.length === 0) {
      return;
    }

    const defaultSidecar = sidecars.find((sidecar) => sidecar.defaultOpen);
    if (defaultSidecar) {
      setActiveKey(sidecarKey(defaultSidecar));
    }
    defaultAppliedRef.current = true;
  }, [sidecars]);

  useEffect(() => {
    if (sidecars.length === 0) {
      setActiveKey(null);
      defaultAppliedRef.current = false;
    }
  }, [sidecars.length]);

  useEffect(() => {
    defaultAppliedRef.current = false;
  }, [registryVersion]);

  const toggleSidecar = useCallback((sidecar: RegisteredSidecar) => {
    const key = sidecarKey(sidecar);
    setActiveKey((current) => (current === key ? null : key));
  }, []);

  const closeSidecar = useCallback(() => setActiveKey(null), []);

  const value = useMemo(
    () => ({
      sidecars,
      activeKey,
      activeSidecar,
      toggleSidecar,
      closeSidecar,
      hostContext,
    }),
    [sidecars, activeKey, activeSidecar, toggleSidecar, closeSidecar, hostContext]
  );

  return <SidecarContext.Provider value={value}>{children}</SidecarContext.Provider>;
}

export function ClientExtensionSidecarControls() {
  const { sidecars, activeKey, toggleSidecar, activeSidecar } = useSidecarContext();

  if (sidecars.length === 0) {
    return null;
  }

  return (
    <div
      className={cn('absolute flex flex-col gap-1 right-3 top-[11px]', activeSidecar && 'mr-1')}
      style={{ zIndex: 100 }}
    >
      {sidecars.map((sidecar) => {
        const key = sidecarKey(sidecar);
        const isActive = activeKey === key;
        return (
          <Button
            key={key}
            type="button"
            variant={isActive ? 'secondary' : 'ghost'}
            size="xs"
            title={sidecar.label}
            aria-label={sidecar.label}
            onClick={() => toggleSidecar(sidecar)}
            className="no-drag"
          >
            <PanelRight className="h-4 w-4" />
            <span className="sr-only">{sidecar.label}</span>
          </Button>
        );
      })}
    </div>
  );
}

function ClientExtensionSidecarContent({
  sidecar,
  hostContext,
  onClose,
}: {
  sidecar: RegisteredSidecar;
  hostContext: ReturnType<typeof useExtensionHostContext>;
  onClose: () => void;
}) {
  const intl = useIntl();
  const { getExtensionFrameDocument, registryVersion } = useClientExtensions();
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const [html, setHtml] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoadError(null);
    setHtml(null);

    void getExtensionFrameDocument(sidecar.extensionId).then((content) => {
      if (cancelled) {
        return;
      }
      if (!content) {
        setLoadError(intl.formatMessage(i18n.loadFailed, { extensionId: sidecar.extensionId }));
        return;
      }
      setHtml(content);
    });

    return () => {
      cancelled = true;
    };
  }, [getExtensionFrameDocument, intl, registryVersion, sidecar.extensionId]);

  const handleExtensionMessage = useCallback(
    (event: MessageEvent) => {
      if (event.source !== iframeRef.current?.contentWindow) {
        return;
      }
      const message = parseExtensionToHostMessage(event.data);
      if (!message) {
        return;
      }

      switch (message.type) {
        case 'grc/ui/showMessage':
          toastService.success({
            title: sidecar.label,
            msg: message.text,
          });
          break;
        default:
          break;
      }
    },
    [sidecar.label]
  );

  useWindowMessage(handleExtensionMessage);

  const notifyActivate = useCallback(() => {
    if (!iframeRef.current?.contentWindow) {
      return;
    }

    const message: HostToExtensionMessage = {
      type: 'grc/activate',
      viewId: sidecar.id,
      viewKind: 'sidecar',
      context: hostContext,
    };
    iframeRef.current.contentWindow.postMessage(message, '*');
  }, [hostContext, sidecar.id]);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex items-center justify-between border-b border-border-primary px-3 py-2">
        <span className="text-sm font-medium text-text-primary">{sidecar.label}</span>
        <Button
          type="button"
          variant="ghost"
          size="xs"
          onClick={onClose}
          aria-label={intl.formatMessage(i18n.close)}
        >
          <X className="h-4 w-4" />
        </Button>
      </div>
      <div className="flex-1 min-h-0">
        {loadError ? (
          <div className="p-4 text-xs text-text-secondary">{loadError}</div>
        ) : !html ? (
          <div className="p-4 text-xs text-text-secondary">{intl.formatMessage(i18n.loading)}</div>
        ) : (
          <iframe
            key={`${registryVersion}:${sidecar.extensionId}:${sidecar.id}`}
            ref={iframeRef}
            title={sidecar.label}
            sandbox={PLUGIN_FRAME_SANDBOX}
            srcDoc={html}
            onLoad={notifyActivate}
            className="h-full w-full border-0 bg-background-primary"
          />
        )}
      </div>
    </div>
  );
}

export function ClientExtensionSidecarPanel() {
  const { activeSidecar, closeSidecar, hostContext, sidecars } = useSidecarContext();
  const isOpen = activeSidecar !== null;

  if (sidecars.length === 0) {
    return null;
  }

  return (
    <motion.div
      initial={false}
      animate={{ width: isOpen ? NAV_DIMENSIONS.SIDECAR_WIDTH : 0 }}
      transition={{ type: 'spring', stiffness: 400, damping: 40 }}
      className="relative flex-shrink-0 overflow-hidden h-full p-2 pl-0"
    >
      {activeSidecar && (
        <div className="h-full w-full overflow-hidden rounded-xl border border-border-primary bg-background-primary">
          <ClientExtensionSidecarContent
            sidecar={activeSidecar}
            hostContext={hostContext}
            onClose={closeSidecar}
          />
        </div>
      )}
    </motion.div>
  );
}
