/**
 * McpAppRenderer — Renders interactive MCP App UIs inside a sandboxed iframe.
 *
 * This component implements the host side of the MCP Apps protocol using
 * @mcp-ui/client protocol primitives. It handles resource fetching, sandbox
 * proxy setup, CSP enforcement, and bidirectional communication with guest apps.
 *
 * Protocol references:
 * - MCP Apps Extension (ext-apps): https://github.com/modelcontextprotocol/ext-apps
 * - MCP-UI Client SDK: https://github.com/idosal/mcp-ui
 * - App Bridge types: @modelcontextprotocol/ext-apps/app-bridge
 *
 * Display modes:
 * - "inline" | "fullscreen" | "pip" — standard MCP display modes
 * - "standalone" — Goose-specific mode for dedicated Electron windows
 */

import {
  AppBridge,
  PostMessageTransport,
  type AppInfo,
  type RequestHandlerExtra,
  type SandboxConfig,
} from '@mcp-ui/client';
import type {
  McpUiDisplayMode,
  McpUiHostCapabilities,
  McpUiHostContext,
  McpUiResourceCsp,
  McpUiResourcePermissions,
  McpUiSizeChangedNotification,
} from '@modelcontextprotocol/ext-apps/app-bridge';
import type { CallToolResult, JSONRPCRequest, Tool } from '@modelcontextprotocol/sdk/types.js';
import { Maximize2, PictureInPicture2, X } from 'lucide-react';
import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from 'react';
import { callMcpAppTool, readMcpAppResource } from '../../acp/mcp-apps';
import { httpBaseFromAcpWebSocketUrl, isLoopbackAcpWebSocketUrl } from '../../acp/url';
import { getCachedTools } from './toolsCache';
import { AppEvents } from '../../constants/events';
import { useTheme } from '../../contexts/ThemeContext';
import { cn } from '../../utils';
import { errorMessage } from '../../utils/conversionUtils';
import { defineMessages, useIntl } from '../../i18n';
import FlyingBird from '../FlyingBird';
import { formatExtensionName } from '../settings/extensions/subcomponents/ExtensionList';
import { getContainerDimensions } from './containerDimensions';
import {
  GooseDisplayMode,
  SandboxPermissions,
  McpAppToolCancelled,
  McpAppToolInput,
  McpAppToolInputPartial,
  OnDisplayModeChange,
} from './types';
import { useDisplayMode, AVAILABLE_DISPLAY_MODES } from './useDisplayMode';
import {
  PIP_FRAME_INSET,
  PIP_RESIZE_HANDLES,
  PIP_TITLE_BAR_HEIGHT,
  pipFrameRect,
  pipResizeZoneRect,
  type PipResizeHandle,
} from './pipGeometry';

const i18n = defineMessages({
  appFallbackTitle: {
    id: 'mcpAppRenderer.appFallbackTitle',
    defaultMessage: 'App',
  },
  pictureInPicture: {
    id: 'mcpAppRenderer.pictureInPicture',
    defaultMessage: 'Picture-in-Picture',
  },
  exitFullscreenTitle: {
    id: 'mcpAppRenderer.exitFullscreenTitle',
    defaultMessage: 'Exit fullscreen (Esc)',
  },
  exitFullscreen: {
    id: 'mcpAppRenderer.exitFullscreen',
    defaultMessage: 'Exit fullscreen',
  },
  fullscreen: {
    id: 'mcpAppRenderer.fullscreen',
    defaultMessage: 'Fullscreen',
  },
  close: {
    id: 'mcpAppRenderer.close',
    defaultMessage: 'Close',
  },
  movePipWindow: {
    id: 'mcpAppRenderer.movePipWindow',
    defaultMessage: 'Move Picture-in-Picture window',
  },
  resizePipWindow: {
    id: 'mcpAppRenderer.resizePipWindow',
    defaultMessage: 'Resize Picture-in-Picture window',
  },
  playingInPip: {
    id: 'mcpAppRenderer.playingInPip',
    defaultMessage: 'Playing in Picture-in-Picture',
  },
  invalidUrl: {
    id: 'mcpAppRenderer.invalidUrl',
    defaultMessage: 'Invalid URL',
  },
  failedToLoadResource: {
    id: 'mcpAppRenderer.failedToLoadResource',
    defaultMessage: 'Failed to load resource',
  },
  failedToInitSandbox: {
    id: 'mcpAppRenderer.failedToInitSandbox',
    defaultMessage: 'Failed to initialize sandbox proxy',
  },
});

const DEFAULT_IFRAME_HEIGHT = 200;
const FULLSCREEN_HEADER_HEIGHT = 48;
// Matches the panel's rounded-xl radius so the title bar fills the corner gap.
const PIP_TITLE_BAR_OVERLAP = 12;
const DEFAULT_SANDBOX_PERMISSIONS = 'allow-scripts allow-same-origin allow-forms';

async function fetchMcpAppProxyUrl(csp: McpUiResourceCsp | null): Promise<string | null> {
  try {
    const acpUrl = await window.electron.getAcpUrl();
    const secretKey = await window.electron.getSecretKey();

    if (!acpUrl || !secretKey) {
      console.error('[McpAppRenderer] Failed to get ACP URL or secret key');
      return null;
    }

    if (!isLoopbackAcpWebSocketUrl(acpUrl)) {
      console.error('[McpAppRenderer] MCP app proxy is only supported for loopback ACP backends');
      return null;
    }

    const httpBase = httpBaseFromAcpWebSocketUrl(acpUrl).replace(/\/+$/, '');
    const proxyUrl = new URL(`${httpBase}/mcp-app-proxy`);
    proxyUrl.searchParams.set('secret', secretKey);

    if (csp?.connectDomains?.length) {
      proxyUrl.searchParams.set('connect_domains', csp.connectDomains.join(','));
    }
    if (csp?.resourceDomains?.length) {
      proxyUrl.searchParams.set('resource_domains', csp.resourceDomains.join(','));
    }
    if (csp?.frameDomains?.length) {
      proxyUrl.searchParams.set('frame_domains', csp.frameDomains.join(','));
    }
    if (csp?.baseUriDomains?.length) {
      proxyUrl.searchParams.set('base_uri_domains', csp.baseUriDomains.join(','));
    }

    return proxyUrl.toString();
  } catch (error) {
    console.error('[McpAppRenderer] Error fetching MCP App Proxy URL:', error);
    return null;
  }
}

interface McpAppRendererProps {
  resourceUri: string;
  extensionName: string;
  toolName?: string;
  sessionId?: string | null;
  toolInput?: McpAppToolInput;
  toolInputPartial?: McpAppToolInputPartial;
  toolResult?: CallToolResult;
  toolCancelled?: McpAppToolCancelled;
  append?: (text: string) => void;
  displayMode?: GooseDisplayMode;
  cachedHtml?: string;
  onDisplayModeChange?: OnDisplayModeChange;
}

interface ResourceMeta {
  csp: McpUiResourceCsp | null;
  permissions: SandboxPermissions | null;
  prefersBorder: boolean;
}

const DEFAULT_META: ResourceMeta = { csp: null, permissions: null, prefersBorder: true };

type FallbackRequestHandler = {
  fallbackRequestHandler?: (
    request: JSONRPCRequest,
    extra: RequestHandlerExtra
  ) => Promise<Record<string, unknown>>;
};

interface GooseAppFrameProps {
  html: string;
  sandbox: SandboxConfig;
  hostContext: McpUiHostContext;
  toolInput?: Record<string, unknown>;
  toolInputPartial?: Record<string, unknown>;
  toolResult?: CallToolResult;
  toolCancelled?: boolean;
  onMessage: (params: {
    content: Array<{ type: string; text?: string }>;
  }) => Promise<Record<string, unknown>>;
  onOpenLink: (params: {
    url: string;
  }) => Promise<{ status: 'success' | 'error'; message?: string }>;
  onCallTool: (params: {
    name: string;
    arguments?: Record<string, unknown>;
  }) => Promise<CallToolResult>;
  onReadResource: (params: { uri: string }) => Promise<{
    contents: Array<{ uri: string; text: string; mimeType?: string }>;
  }>;
  onLoggingMessage: (params: { level?: string; logger?: string; data?: unknown }) => void;
  onFallbackRequest: (
    request: JSONRPCRequest,
    extra: RequestHandlerExtra
  ) => Promise<Record<string, unknown>>;
  onSizeChanged?: (params: McpUiSizeChangedNotification['params']) => void;
  onInitialized?: (appInfo: AppInfo) => void;
  onError?: (error: Error) => void;
}

const SANDBOX_PROXY_READY_METHOD = 'ui/notifications/sandbox-proxy-ready';

// In PiP the iframe must fill the window even when the guest's content is
// shorter, otherwise the window's own background shows below the app.
const PIP_RESIZE_CURSOR: Record<PipResizeHandle, string> = {
  top: 'cursor-ns-resize',
  bottom: 'cursor-ns-resize',
  left: 'cursor-ew-resize',
  right: 'cursor-ew-resize',
  'top-left': 'cursor-nwse-resize',
  'bottom-right': 'cursor-nwse-resize',
  'top-right': 'cursor-nesw-resize',
  'bottom-left': 'cursor-nesw-resize',
};

function iframeHeightFor(guestHeight: number, hostContext: McpUiHostContext): number {
  const dimensions = hostContext.containerDimensions;
  if (hostContext.displayMode !== 'pip' || !dimensions || !('maxHeight' in dimensions)) {
    return guestHeight;
  }
  return Math.max(guestHeight, dimensions.maxHeight ?? 0);
}

function GooseAppFrame({
  html,
  sandbox,
  hostContext,
  toolInput,
  toolInputPartial,
  toolResult,
  toolCancelled,
  onMessage,
  onOpenLink,
  onCallTool,
  onReadResource,
  onLoggingMessage,
  onFallbackRequest,
  onSizeChanged,
  onInitialized,
  onError,
}: GooseAppFrameProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const iframeRef = useRef<HTMLIFrameElement | null>(null);
  const guestHeightRef = useRef<number | null>(null);
  const bridgeRef = useRef<AppBridge | null>(null);
  const [connected, setConnected] = useState(false);
  const [initialized, setInitialized] = useState(false);
  const hostContextRef = useRef(hostContext);
  const onMessageRef = useRef(onMessage);
  const onOpenLinkRef = useRef(onOpenLink);
  const onCallToolRef = useRef(onCallTool);
  const onReadResourceRef = useRef(onReadResource);
  const onLoggingMessageRef = useRef(onLoggingMessage);
  const onFallbackRequestRef = useRef(onFallbackRequest);
  const onSizeChangedRef = useRef(onSizeChanged);
  const onInitializedRef = useRef(onInitialized);
  const onErrorRef = useRef(onError);

  useEffect(() => {
    hostContextRef.current = hostContext;
    onMessageRef.current = onMessage;
    onOpenLinkRef.current = onOpenLink;
    onCallToolRef.current = onCallTool;
    onReadResourceRef.current = onReadResource;
    onLoggingMessageRef.current = onLoggingMessage;
    onFallbackRequestRef.current = onFallbackRequest;
    onSizeChangedRef.current = onSizeChanged;
    onInitializedRef.current = onInitialized;
    onErrorRef.current = onError;
  });

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    setConnected(false);
    setInitialized(false);

    const capabilities: McpUiHostCapabilities = {
      openLinks: {},
      serverTools: {},
      serverResources: {},
      logging: {},
      message: { text: {} },
    };
    const bridge = new AppBridge(null, { name: 'MCP-UI Host', version: '1.0.0' }, capabilities, {
      hostContext: hostContextRef.current,
    });
    bridge.onmessage = (params) => onMessageRef.current(params);
    bridge.onopenlink = (params) => onOpenLinkRef.current(params);
    bridge.onloggingmessage = (params) => onLoggingMessageRef.current(params);
    bridge.oncalltool = (params) => onCallToolRef.current(params);
    bridge.onreadresource = (params) => onReadResourceRef.current(params);
    (bridge as FallbackRequestHandler).fallbackRequestHandler = (request, extra) =>
      onFallbackRequestRef.current(request, extra);

    const iframe = document.createElement('iframe');
    iframe.style.width = '100%';
    iframe.style.height = '600px';
    iframe.style.border = 'none';
    iframe.style.backgroundColor = 'transparent';
    iframe.setAttribute('sandbox', sandbox.permissions || DEFAULT_SANDBOX_PERMISSIONS);

    let active = true;
    let settled = false;
    const cleanupReadyListener = () => {
      window.removeEventListener('message', handleReadyMessage);
      iframe.removeEventListener('error', handleFrameError);
      clearTimeout(timeout);
    };
    const fail = (error: Error) => {
      if (settled) return;
      settled = true;
      cleanupReadyListener();
      if (active) {
        onErrorRef.current?.(error);
      }
    };
    const ready = () => {
      if (settled) return;
      settled = true;
      cleanupReadyListener();
      void connectBridge();
    };
    function handleReadyMessage(event: MessageEvent) {
      if (event.source !== iframe.contentWindow) return;
      if (event.data?.method === SANDBOX_PROXY_READY_METHOD) {
        ready();
      }
    }
    function handleFrameError() {
      fail(new Error('Failed to load sandbox proxy iframe'));
    }
    const timeout = window.setTimeout(() => {
      fail(new Error('Timed out waiting for sandbox proxy iframe to be ready'));
    }, 10_000);
    const connectBridge = async () => {
      if (!active || !iframe.contentWindow) return;
      try {
        bridge.onsizechange = (params) => {
          onSizeChangedRef.current?.(params);
          if (params.width !== undefined) {
            iframe.style.width = `${params.width}px`;
          }
          if (params.height !== undefined) {
            guestHeightRef.current = params.height;
            iframe.style.height = `${iframeHeightFor(params.height, hostContextRef.current)}px`;
          }
        };
        bridge.oninitialized = () => {
          if (!active) return;
          setInitialized(true);
          onInitializedRef.current?.({
            appVersion: bridge.getAppVersion(),
            appCapabilities: bridge.getAppCapabilities(),
          });
        };
        await bridge.connect(new PostMessageTransport(iframe.contentWindow, iframe.contentWindow));
        if (!active) return;
        bridgeRef.current = bridge;
        setConnected(true);
      } catch (error) {
        if (!active) return;
        onErrorRef.current?.(error instanceof Error ? error : new Error(String(error)));
      }
    };

    window.addEventListener('message', handleReadyMessage);
    iframe.addEventListener('error', handleFrameError);
    container.replaceChildren(iframe);
    iframeRef.current = iframe;
    iframe.src = sandbox.url.href;

    return () => {
      active = false;
      cleanupReadyListener();
      if (iframeRef.current === iframe) {
        iframeRef.current = null;
      }
      if (bridgeRef.current === bridge) {
        bridgeRef.current = null;
      }
      bridge.close();
      iframe.remove();
    };
  }, [sandbox.permissions, sandbox.url.href]);

  useEffect(() => {
    const bridge = bridgeRef.current;
    if (!connected || !bridge) return;
    void Promise.resolve(bridge.sendSandboxResourceReady({ html, csp: sandbox.csp })).catch(
      (error: unknown) => {
        onErrorRef.current?.(error instanceof Error ? error : new Error(String(error)));
      }
    );
  }, [connected, html, sandbox.csp]);

  useEffect(() => {
    const bridge = bridgeRef.current;
    if (connected && initialized && toolInput && bridge) {
      void bridge.sendToolInput({ arguments: toolInput });
    }
  }, [connected, initialized, toolInput]);

  useEffect(() => {
    const bridge = bridgeRef.current;
    if (connected && initialized && toolResult && bridge) {
      void bridge.sendToolResult(toolResult);
    }
  }, [connected, initialized, toolResult]);

  useEffect(() => {
    const bridge = bridgeRef.current;
    if (initialized && bridge) {
      bridge.setHostContext(hostContext);
    }
  }, [initialized, hostContext]);

  // Re-apply the PiP floor when the window is resized or the mode changes.
  useEffect(() => {
    const iframe = iframeRef.current;
    const guestHeight = guestHeightRef.current;
    if (!iframe || guestHeight === null) return;
    iframe.style.height = `${iframeHeightFor(guestHeight, hostContext)}px`;
  }, [hostContext]);

  useEffect(() => {
    const bridge = bridgeRef.current;
    if (initialized && toolInputPartial && bridge) {
      void bridge.sendToolInputPartial({ arguments: toolInputPartial });
    }
  }, [initialized, toolInputPartial]);

  useEffect(() => {
    const bridge = bridgeRef.current;
    if (initialized && toolCancelled && bridge) {
      void bridge.sendToolCancelled({});
    }
  }, [initialized, toolCancelled]);

  return <div ref={containerRef} className="flex h-full w-full flex-col" />;
}

// Lifecycle: idle → loading_resource → loading_sandbox → ready
// Any state can transition to error. The sandbox URL is fetched only once
// to prevent iframe recreation (which would cause the app to lose state).
type AppState =
  | { status: 'idle' }
  | { status: 'loading_resource'; html: string | null; meta: ResourceMeta }
  | { status: 'loading_sandbox'; html: string; meta: ResourceMeta }
  | {
      status: 'ready';
      html: string;
      meta: ResourceMeta;
      sandboxUrl: URL;
      sandboxCsp: McpUiResourceCsp | null;
    }
  | { status: 'error'; message: string; html: string | null; meta: ResourceMeta };

type AppAction =
  | { type: 'FETCH_RESOURCE' }
  | { type: 'RESOURCE_LOADED'; html: string | null; meta: ResourceMeta }
  | { type: 'RESOURCE_FAILED'; message: string }
  | { type: 'SANDBOX_READY'; sandboxUrl: string; sandboxCsp: McpUiResourceCsp | null }
  | { type: 'SANDBOX_FAILED'; message: string }
  | { type: 'ERROR'; message: string };

function getMeta(state: AppState): ResourceMeta {
  return state.status === 'idle' ? DEFAULT_META : state.meta;
}

function getHtml(state: AppState): string | null {
  return state.status === 'idle' ? null : state.html;
}

function appReducer(state: AppState, action: AppAction): AppState {
  const meta = getMeta(state);
  const html = getHtml(state);

  switch (action.type) {
    case 'FETCH_RESOURCE':
      if (state.status === 'ready') return state;
      return { status: 'loading_resource', html, meta };

    case 'RESOURCE_LOADED':
      if (!action.html) {
        return { status: 'loading_resource', html: null, meta: action.meta };
      }
      if (state.status === 'ready') {
        return { ...state, html: action.html, meta: action.meta };
      }
      return { status: 'loading_sandbox', html: action.html, meta: action.meta };

    case 'RESOURCE_FAILED':
      if (html) {
        if (state.status === 'ready') return state;
        return { status: 'loading_sandbox', html, meta };
      }
      return { status: 'error', message: action.message, html: null, meta };

    case 'SANDBOX_READY':
      if (!html) return state;
      return {
        status: 'ready',
        html,
        meta,
        sandboxUrl: new URL(action.sandboxUrl),
        sandboxCsp: action.sandboxCsp,
      };

    case 'SANDBOX_FAILED':
      return { status: 'error', message: action.message, html, meta };

    case 'ERROR':
      return { status: 'error', message: action.message, html, meta };
  }
}

export default function McpAppRenderer({
  resourceUri,
  extensionName,
  toolName,
  sessionId,
  toolInput,
  toolInputPartial,
  toolResult,
  toolCancelled,
  append,
  displayMode = 'inline',
  cachedHtml,
  onDisplayModeChange,
}: McpAppRendererProps) {
  const intl = useIntl();
  const containerRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);

  const dm = useDisplayMode({ displayMode, onDisplayModeChange, containerRef, sessionId });
  const {
    activeDisplayMode,
    effectiveDisplayModes,
    isStandalone,
    isFullscreen,
    isPip,
    isFillsViewport,
    isInline,
    appSupportsFullscreen,
    appSupportsPip,
    appTitle,
    changeDisplayMode,
    inlineHeight,
    pipGeometry,
    pipHandlers,
    pipResizeHandlers,
    fullscreenCloseRef,
  } = dm;

  const { resolvedTheme, mcpHostStyles } = useTheme();

  // Fetch the MCP Tool definition (name, description, inputSchema) for hostContext.toolInfo.
  // Note: the spec also calls for toolInfo.id — the JSON-RPC id of the tools/call request
  // between the MCP client and server. That id is generated internally by rmcp's transport
  // layer and isn't surfaced through the extension manager or message stream to the frontend.
  // Plumbing it would require changes from rmcp → extension_manager → message → SSE → UI.
  const [mcpTool, setMcpTool] = useState<Tool | null>(null);
  const toolDefRef = useRef<Tool | null>(null);
  useEffect(() => {
    if (!sessionId || !toolName || toolDefRef.current) {
      if (toolDefRef.current) setMcpTool(toolDefRef.current);
      return;
    }

    let cancelled = false;
    (async () => {
      const tools = await getCachedTools(sessionId, extensionName || undefined);
      if (cancelled || !tools) return;

      const prefixedName = extensionName ? `${extensionName}__${toolName}` : toolName;
      const match = tools.find((t) => t.name === prefixedName);
      if (match) {
        const tool: Tool = {
          name: toolName,
          description: match.description || undefined,
          inputSchema: (match.inputSchema as Tool['inputSchema']) ?? { type: 'object' as const },
        };
        toolDefRef.current = tool;
        setMcpTool(tool);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [sessionId, toolName, extensionName]);

  // Survive StrictMode remounts — replay cached results instead of re-fetching,
  // which prevents the iframe from being torn down and recreated (visible flicker).
  // Declared before useReducer so the lazy initializer can read them.
  const fetchedDataRef = useRef<{ html: string; meta: ResourceMeta } | null>(null);
  const sandboxUrlRef = useRef<{ url: string; csp: McpUiResourceCsp | null } | null>(null);

  const [state, dispatch] = useReducer(appReducer, undefined, (): AppState => {
    // On StrictMode remount, skip straight to ready if we have all cached data.
    if (fetchedDataRef.current && sandboxUrlRef.current) {
      return {
        status: 'ready',
        html: fetchedDataRef.current.html,
        meta: fetchedDataRef.current.meta,
        sandboxUrl: new URL(sandboxUrlRef.current.url),
        sandboxCsp: sandboxUrlRef.current.csp,
      };
    }
    if (cachedHtml) {
      return { status: 'loading_sandbox', html: cachedHtml, meta: DEFAULT_META };
    }
    return { status: 'idle' };
  });
  const [iframeHeight, setIframeHeight] = useState(DEFAULT_IFRAME_HEIGHT);

  // Restore iframeHeight from the saved snapshot when returning to inline.
  // While in fullscreen/pip, handleSizeChanged ignores size notifications, so
  // iframeHeight may be stale. This ensures the container starts at the correct
  // height the moment the mode flips back to inline.
  useEffect(() => {
    if (isInline) {
      setIframeHeight(inlineHeight);
    }
  }, [isInline, inlineHeight]);

  const effectiveInlineHeight = iframeHeight || DEFAULT_IFRAME_HEIGHT;

  const [containerWidth, setContainerWidth] = useState<number>(0);
  const [containerHeight, setContainerHeight] = useState<number>(0);

  // Fetch the resource from the extension to get HTML and metadata (CSP, permissions, etc.).
  // If cachedHtml is provided we show it immediately; the fetch updates metadata and
  // replaces HTML only if the server returns different content.
  //
  // Retries with exponential backoff when the fetch fails (e.g. the extension hasn't
  // finished loading yet, causing a transient 500). Cached HTML skips retries since
  // the app can render immediately with the cached version.
  useEffect(() => {
    if (!sessionId) return;

    // On StrictMode remount, replay the cached result instead of re-fetching.
    if (fetchedDataRef.current) {
      const { html: cachedResult, meta: cachedMeta } = fetchedDataRef.current;
      dispatch({ type: 'RESOURCE_LOADED', html: cachedResult, meta: cachedMeta });
      return;
    }

    const MAX_RETRIES = 5;
    const BASE_DELAY_MS = 500;
    let cancelled = false;

    const fetchResourceData = async () => {
      dispatch({ type: 'FETCH_RESOURCE' });

      for (let attempt = 0; attempt <= MAX_RETRIES; attempt++) {
        if (cancelled) return;

        try {
          const content = await readMcpAppResource(sessionId, extensionName, resourceUri);

          if (cancelled) return;

          if (content) {
            const rawMeta = content._meta as
              | {
                  ui?: {
                    csp?: McpUiResourceCsp;
                    permissions?: McpUiResourcePermissions;
                    prefersBorder?: boolean;
                  };
                }
              | undefined;

            const resolvedHtml = content.text ?? cachedHtml ?? null;
            const resolvedMeta = {
              csp: rawMeta?.ui?.csp || null,
              // todo: pass permissions to SDK once it supports sendSandboxResourceReady
              // https://github.com/MCP-UI-Org/mcp-ui/issues/180
              permissions: null,
              prefersBorder: rawMeta?.ui?.prefersBorder ?? true,
            };

            if (resolvedHtml) {
              fetchedDataRef.current = { html: resolvedHtml, meta: resolvedMeta };
            }
            dispatch({ type: 'RESOURCE_LOADED', html: resolvedHtml, meta: resolvedMeta });
            return;
          }
        } catch (err) {
          if (cancelled) return;

          const isLastAttempt = attempt === MAX_RETRIES;

          if (!isLastAttempt && !cachedHtml) {
            const delay = BASE_DELAY_MS * Math.pow(2, attempt);
            console.warn(
              `[McpAppRenderer] Resource fetch attempt ${attempt + 1}/${MAX_RETRIES + 1} failed, retrying in ${delay}ms:`,
              err
            );
            await new Promise((resolve) => setTimeout(resolve, delay));
            continue;
          }

          console.error('[McpAppRenderer] Error fetching resource:', err);
          if (cachedHtml) {
            console.warn('Failed to fetch fresh resource, using cached version:', err);
          }
          dispatch({
            type: 'RESOURCE_FAILED',
            message: errorMessage(err, intl.formatMessage(i18n.failedToLoadResource)),
          });
          return;
        }
      }
    };

    fetchResourceData();

    return () => {
      cancelled = true;
    };
  }, [resourceUri, extensionName, sessionId, cachedHtml, intl]);

  // Create the sandbox proxy URL once we have HTML and metadata.
  // On StrictMode remount, reuse the cached URL to avoid recreating the proxy
  // (which would destroy iframe state and cause a visible flicker).
  const pendingCsp = state.status === 'loading_sandbox' ? state.meta.csp : null;
  useEffect(() => {
    if (state.status !== 'loading_sandbox') return;

    if (sandboxUrlRef.current) {
      const { url, csp } = sandboxUrlRef.current;
      dispatch({ type: 'SANDBOX_READY', sandboxUrl: url, sandboxCsp: csp });
      return;
    }

    fetchMcpAppProxyUrl(pendingCsp).then((url) => {
      if (url) {
        sandboxUrlRef.current = { url, csp: pendingCsp };
        dispatch({ type: 'SANDBOX_READY', sandboxUrl: url, sandboxCsp: pendingCsp });
      } else {
        dispatch({ type: 'SANDBOX_FAILED', message: intl.formatMessage(i18n.failedToInitSandbox) });
      }
    });
  }, [state.status, pendingCsp, intl]);

  const handleOpenLink = useCallback(
    async ({ url }: { url: string }) => {
      const result = await window.electron.openExternal(url);
      if (result === 'opened') {
        return { status: 'success' as const };
      }

      return {
        status: 'error' as const,
        message: result === 'cancelled' ? 'User cancelled' : intl.formatMessage(i18n.invalidUrl),
      };
    },
    [intl]
  );

  const handleMessage = useCallback(
    async ({ content }: { content: Array<{ type: string; text?: string }> }) => {
      if (!append) {
        throw new Error('Message handler not available in this context');
      }
      if (!Array.isArray(content)) {
        throw new Error('Invalid message format: content must be an array of ContentBlock');
      }
      const textContent = content.find((block) => block.type === 'text');
      if (!textContent || !textContent.text) {
        throw new Error('Invalid message format: content must contain a text block');
      }
      append(textContent.text);
      window.dispatchEvent(new CustomEvent(AppEvents.SCROLL_CHAT_TO_BOTTOM));
      return {};
    },
    [append]
  );

  const handleCallTool = useCallback(
    async ({
      name,
      arguments: args,
    }: {
      name: string;
      arguments?: Record<string, unknown>;
    }): Promise<CallToolResult> => {
      if (!sessionId) {
        throw new Error('Session not initialized for MCP request');
      }
      return callMcpAppTool(sessionId, extensionName, name, args);
    },
    [sessionId, extensionName]
  );

  const handleReadResource = useCallback(
    async ({ uri }: { uri: string }) => {
      if (!sessionId) {
        throw new Error('Session not initialized for MCP request');
      }
      const data = await readMcpAppResource(sessionId, extensionName, uri);
      if (!data) {
        return { contents: [] };
      }
      return {
        contents: [{ uri: data.uri || uri, text: data.text, mimeType: data.mimeType || undefined }],
      };
    },
    [sessionId, extensionName]
  );

  const handleLoggingMessage = useCallback(
    (_notification: { level?: string; logger?: string; data?: unknown }) => {},
    []
  );

  // Track when we *return* to inline from fullscreen/pip so we can briefly
  // suppress stale size reports. The iframe body reflows from 100vh to natural
  // height, which triggers a cascade of intermediate size-changed notifications
  // that cause a visible "slow shrink" animation.
  const inlineTransitionRef = useRef(false);
  const wasInlineRef = useRef(isInline);
  useEffect(() => {
    const wasInline = wasInlineRef.current;
    wasInlineRef.current = isInline;
    // Only suppress when transitioning *back* to inline, not on initial mount.
    if (!isInline || wasInline) return;
    inlineTransitionRef.current = true;
    const timer = setTimeout(() => {
      inlineTransitionRef.current = false;
    }, 300);
    return () => clearTimeout(timer);
  }, [isInline]);

  const handleSizeChanged = useCallback(
    ({ height }: McpUiSizeChangedNotification['params']) => {
      if (height !== undefined && height > 0 && isInline && !inlineTransitionRef.current) {
        setIframeHeight(height);
      }
    },
    [isInline]
  );

  // Track pixel dimensions for containerDimensions. Width comes from the content
  // element so a PiP scrollbar gutter is not counted; height from the container.
  useEffect(() => {
    const container = containerRef.current;
    const content = contentRef.current;
    if (!container || !content) return;

    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        if (entry.target === content) {
          const width = Math.round(entry.contentRect.width);
          setContainerWidth((prev) => (prev !== width ? width : prev));
        }
        if (entry.target === container) {
          const height = Math.round(entry.contentRect.height);
          setContainerHeight((prev) => (prev !== height ? height : prev));
        }
      }
    });

    observer.observe(container);
    observer.observe(content);
    return () => observer.disconnect();
  }, []);

  const handleFallbackRequest = useCallback(
    async (request: JSONRPCRequest, _extra: RequestHandlerExtra) => {
      return {
        status: 'error' as const,
        message: `Unhandled JSON-RPC method: ${request.method ?? '<unknown>'}`,
      };
    },
    []
  );

  const handleError = useCallback((err: Error) => {
    console.error('[MCP App Error]:', err);
    dispatch({ type: 'ERROR', message: errorMessage(err) });
  }, []);

  const meta = getMeta(state);
  const html = getHtml(state);

  const readyCsp = state.status === 'ready' ? state.sandboxCsp : null;
  const mcpUiCsp = useMemo((): McpUiResourceCsp | undefined => {
    if (!readyCsp) return undefined;
    return {
      connectDomains: readyCsp.connectDomains ?? undefined,
      resourceDomains: readyCsp.resourceDomains ?? undefined,
      frameDomains: readyCsp.frameDomains ?? undefined,
      baseUriDomains: readyCsp.baseUriDomains ?? undefined,
    };
  }, [readyCsp]);

  const readySandboxUrl = state.status === 'ready' ? state.sandboxUrl : null;
  const sandboxConfig = useMemo(() => {
    if (!readySandboxUrl) return null;
    return {
      url: readySandboxUrl,
      permissions: meta.permissions || DEFAULT_SANDBOX_PERMISSIONS,
      csp: mcpUiCsp,
    };
  }, [readySandboxUrl, meta.permissions, mcpUiCsp]);

  const hostContext = useMemo((): McpUiHostContext => {
    const context: McpUiHostContext = {
      toolInfo: mcpTool ? { tool: mcpTool } : undefined,
      theme: resolvedTheme,
      styles: mcpHostStyles,
      displayMode: activeDisplayMode as McpUiDisplayMode,
      availableDisplayModes: isStandalone
        ? [activeDisplayMode as McpUiDisplayMode]
        : effectiveDisplayModes.length > 0
          ? effectiveDisplayModes
          : AVAILABLE_DISPLAY_MODES,
      containerDimensions: getContainerDimensions(
        activeDisplayMode,
        containerWidth,
        isFullscreen ? containerHeight - FULLSCREEN_HEADER_HEIGHT : containerHeight
      ),
      locale: navigator.language,
      timeZone: Intl.DateTimeFormat().resolvedOptions().timeZone,
      userAgent: navigator.userAgent,
      platform: 'desktop',
      deviceCapabilities: {
        touch: navigator.maxTouchPoints > 0,
        hover: window.matchMedia('(hover: hover)').matches,
      },
      safeAreaInsets: {
        top: 0,
        right: 0,
        bottom: 0,
        left: 0,
      },
    };

    return context;
  }, [
    resolvedTheme,
    mcpHostStyles,
    activeDisplayMode,
    isFullscreen,
    isStandalone,
    containerWidth,
    containerHeight,
    effectiveDisplayModes,
    mcpTool,
  ]);

  const isError = state.status === 'error';
  const isReady = state.status === 'ready';

  const renderContent = () => {
    if (isError) {
      return (
        <div className="p-4 text-red-700 dark:text-red-300">
          Failed to load MCP app: {state.message}
        </div>
      );
    }

    if (!isReady) {
      return (
        <div className="relative flex h-full w-full items-center justify-center overflow-hidden rounded bg-black/[0.03] dark:bg-white/[0.03]">
          <div
            className="absolute inset-0 animate-shimmer"
            style={{
              animationDuration: '2s',
              background:
                'linear-gradient(90deg, transparent 0%, rgba(128,128,128,0.08) 40%, rgba(128,128,128,0.12) 50%, rgba(128,128,128,0.08) 60%, transparent 100%)',
            }}
          />
          <FlyingBird className="relative z-10 scale-200 opacity-30" cycleInterval={120} />
        </div>
      );
    }

    if (!sandboxConfig) return null;

    return (
      <GooseAppFrame
        sandbox={sandboxConfig}
        html={html ?? ''}
        hostContext={hostContext}
        toolInput={toolInput?.arguments}
        toolInputPartial={toolInputPartial?.arguments}
        toolResult={toolResult}
        toolCancelled={!!toolCancelled}
        onMessage={handleMessage}
        onOpenLink={handleOpenLink}
        onCallTool={handleCallTool}
        onReadResource={handleReadResource}
        onLoggingMessage={handleLoggingMessage}
        onFallbackRequest={handleFallbackRequest}
        onSizeChanged={handleSizeChanged}
        onError={handleError}
      />
    );
  };

  const showControls =
    !isStandalone && !isError && (appSupportsFullscreen || appSupportsPip || isFullscreen || isPip);

  const fullscreenTitle = useMemo(() => {
    if (appTitle) return appTitle;
    if (extensionName) return formatExtensionName(extensionName);
    return intl.formatMessage(i18n.appFallbackTitle);
  }, [appTitle, extensionName, intl]);

  const renderFullscreenHeader = () => (
    <div
      className="flex shrink-0 items-center border-b border-border-primary bg-background-primary px-3"
      style={{ height: `${FULLSCREEN_HEADER_HEIGHT}px` }}
    >
      <div className="min-w-0 flex-1" />
      <span className="truncate px-4 text-sm font-medium text-text-secondary">
        {fullscreenTitle}
      </span>
      <div className="flex flex-1 items-center justify-end gap-1">
        {appSupportsPip && (
          <button
            onClick={() => changeDisplayMode('pip')}
            className="no-drag cursor-pointer rounded-md p-1.5 text-text-secondary transition-colors hover:bg-black/10 hover:text-text-primary dark:hover:bg-white/10"
            title={intl.formatMessage(i18n.pictureInPicture)}
            aria-label={intl.formatMessage(i18n.pictureInPicture)}
          >
            <PictureInPicture2 size={16} />
          </button>
        )}
        <button
          ref={fullscreenCloseRef}
          onClick={() => changeDisplayMode('inline')}
          className="no-drag cursor-pointer rounded-md p-1.5 text-text-secondary transition-colors hover:bg-black/10 hover:text-text-primary dark:hover:bg-white/10"
          title={intl.formatMessage(i18n.exitFullscreenTitle)}
          aria-label={intl.formatMessage(i18n.exitFullscreen)}
        >
          <X size={16} />
        </button>
      </div>
    </div>
  );

  const renderDisplayModeControls = () => {
    if (!showControls) return null;

    // Fullscreen controls are rendered by renderFullscreenHeader instead.
    if (activeDisplayMode === 'fullscreen') return null;

    // PiP controls are rendered by renderPipControls instead.
    if (activeDisplayMode === 'pip') return null;

    // Inline mode — show controls on hover or keyboard focus
    return (
      <div className="absolute top-2 right-2 z-10 flex gap-1 opacity-0 transition-opacity group-hover/mcp-app:opacity-100 focus-within:opacity-100">
        {appSupportsFullscreen && (
          <button
            onClick={() => changeDisplayMode('fullscreen')}
            className="cursor-pointer rounded-md bg-black/40 p-1.5 text-white backdrop-blur-sm transition-opacity hover:bg-black/60"
            title={intl.formatMessage(i18n.fullscreen)}
            aria-label={intl.formatMessage(i18n.fullscreen)}
          >
            <Maximize2 size={14} />
          </button>
        )}
        {appSupportsPip && (
          <button
            onClick={() => changeDisplayMode('pip')}
            className="cursor-pointer rounded-md bg-black/40 p-1.5 text-white backdrop-blur-sm transition-opacity hover:bg-black/60"
            title={intl.formatMessage(i18n.pictureInPicture)}
            aria-label={intl.formatMessage(i18n.pictureInPicture)}
          >
            <PictureInPicture2 size={14} />
          </button>
        )}
      </div>
    );
  };

  // Single stable container — CSS switches between inline/fullscreen/pip positioning.
  // The iframe is never unmounted, preserving app state across mode changes.
  const containerClasses = cn(
    'mcp-app-container bg-background-primary [&_iframe]:!w-full',
    isFillsViewport && 'fixed inset-0 z-[1000] overflow-hidden [&_iframe]:!h-full',
    isPip &&
      'pointer-events-auto absolute z-10 overflow-hidden rounded-xl border border-border-primary shadow-2xl',
    isInline && 'group/mcp-app relative overflow-hidden',
    isInline && !isError && 'mt-6 mb-2',
    isInline && !isError && meta.prefersBorder && 'border border-border-primary rounded-lg',
    isError && 'border border-red-500 rounded-lg bg-red-50 dark:bg-red-900/20'
  );

  const containerStyle: React.CSSProperties = {
    ...(isFillsViewport
      ? {}
      : isPip
        ? {
            top: `${PIP_FRAME_INSET.top}px`,
            left: `${PIP_FRAME_INSET.left}px`,
            width: `${pipGeometry.width}px`,
            height: `${pipGeometry.height}px`,
          }
        : {
            width: '100%',
            height: `${effectiveInlineHeight}px`,
          }),
  };

  const pipFrame = pipFrameRect(pipGeometry);
  const pipFrameStyle: React.CSSProperties | undefined = isPip
    ? {
        width: `${pipFrame.width}px`,
        height: `${pipFrame.height}px`,
        right: `${pipFrame.right}px`,
        bottom: `${pipFrame.bottom}px`,
      }
    : undefined;

  // Title bar above the panel, styled like the fullscreen header. A drag layer
  // fills the bar so the whole bar moves the window, with the buttons layered
  // over it. The bar only takes pointer events while the PiP is hovered or
  // focused, so it never intercepts clicks meant for the chat behind it. It
  // extends under the panel's rounded top corners so the two join cleanly.
  const renderPipControls = () => (
    <div
      className="pointer-events-none absolute flex translate-y-1 select-none items-center rounded-t-xl border border-b-0 border-border-primary bg-background-primary px-2 opacity-0 transition-[opacity,transform] duration-200 ease-out group-hover/pip:pointer-events-auto group-hover/pip:translate-y-0 group-hover/pip:opacity-100 focus-within:pointer-events-auto focus-within:translate-y-0 focus-within:opacity-100 motion-reduce:transition-none"
      style={{
        left: `${PIP_FRAME_INSET.left}px`,
        right: `${PIP_FRAME_INSET.right}px`,
        top: `${PIP_FRAME_INSET.top - PIP_TITLE_BAR_HEIGHT}px`,
        height: `${PIP_TITLE_BAR_HEIGHT + PIP_TITLE_BAR_OVERLAP}px`,
        paddingBottom: `${PIP_TITLE_BAR_OVERLAP}px`,
      }}
    >
      <div
        role="button"
        tabIndex={0}
        aria-label={intl.formatMessage(i18n.movePipWindow)}
        aria-keyshortcuts="ArrowUp ArrowDown ArrowLeft ArrowRight"
        className="absolute inset-0 cursor-grab rounded-t-xl outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-border-active active:cursor-grabbing"
        onPointerDown={pipHandlers.onPointerDown}
        onPointerMove={pipHandlers.onPointerMove}
        onPointerUp={pipHandlers.onPointerUp}
        onLostPointerCapture={pipHandlers.onLostPointerCapture}
        onKeyDown={pipHandlers.onKeyDown}
      />
      <div className="pointer-events-none min-w-0 flex-1" />
      <span className="pointer-events-none truncate px-3 text-xs font-medium text-text-secondary">
        {fullscreenTitle}
      </span>
      <div className="pointer-events-none relative flex flex-1 items-center justify-end gap-0.5">
        {appSupportsFullscreen && (
          <button
            onClick={() => changeDisplayMode('fullscreen')}
            className="pointer-events-auto cursor-pointer rounded-md p-1 text-text-secondary transition-colors hover:bg-black/10 hover:text-text-primary dark:hover:bg-white/10"
            title={intl.formatMessage(i18n.fullscreen)}
            aria-label={intl.formatMessage(i18n.fullscreen)}
          >
            <Maximize2 size={14} />
          </button>
        )}
        <button
          onClick={() => changeDisplayMode('inline')}
          className="pointer-events-auto cursor-pointer rounded-md p-1 text-text-secondary transition-colors hover:bg-black/10 hover:text-text-primary dark:hover:bg-white/10"
          title={intl.formatMessage(i18n.close)}
          aria-label={intl.formatMessage(i18n.close)}
        >
          <X size={14} />
        </button>
      </div>
    </div>
  );

  const renderPipResizeZones = () =>
    PIP_RESIZE_HANDLES.map((handle) => {
      const handlers = pipResizeHandlers[handle];
      const zone = pipResizeZoneRect(handle, pipGeometry);
      // Arrow keys on one handle can reach any size, so only the bottom-right
      // corner is exposed to keyboard and screen readers.
      const isKeyboardHandle = handle === 'bottom-right';
      return (
        <div
          key={handle}
          role={isKeyboardHandle ? 'button' : undefined}
          tabIndex={isKeyboardHandle ? 0 : undefined}
          aria-hidden={isKeyboardHandle ? undefined : true}
          aria-label={isKeyboardHandle ? intl.formatMessage(i18n.resizePipWindow) : undefined}
          aria-keyshortcuts={
            isKeyboardHandle ? 'ArrowUp ArrowDown ArrowLeft ArrowRight' : undefined
          }
          className={cn(
            'pointer-events-auto absolute z-20 rounded-sm outline-none focus-visible:ring-2 focus-visible:ring-border-active',
            PIP_RESIZE_CURSOR[handle]
          )}
          style={{
            left: `${zone.left}px`,
            top: `${zone.top}px`,
            width: `${zone.width}px`,
            height: `${zone.height}px`,
          }}
          onPointerDown={handlers.onPointerDown}
          onPointerMove={handlers.onPointerMove}
          onPointerUp={handlers.onPointerUp}
          onLostPointerCapture={handlers.onLostPointerCapture}
          onKeyDown={isKeyboardHandle ? handlers.onKeyDown : undefined}
        />
      );
    });

  return (
    <>
      {/* Placeholder in chat flow when app is detached (fullscreen or pip) */}
      {isFullscreen && (
        <div
          className="invisible mt-6 mb-2"
          style={{ width: '100%', height: `${inlineHeight}px` }}
        />
      )}
      {isPip && (
        <div
          className="mt-6 mb-2 flex items-center justify-center rounded-lg border border-dashed border-border-primary bg-black/[0.02] dark:bg-white/[0.02]"
          style={{ width: '100%', height: `${inlineHeight}px` }}
        >
          <button
            onClick={() => changeDisplayMode('inline')}
            className="cursor-pointer flex items-center gap-2 rounded-md px-3 py-1.5 text-xs text-text-secondary transition-colors hover:bg-black/5 hover:text-text-primary dark:hover:bg-white/5"
          >
            <PictureInPicture2 size={14} />
            <span>{intl.formatMessage(i18n.playingInPip)}</span>
          </button>
        </div>
      )}

      {/* Stable app container — never unmounted, only repositioned via CSS.
          In PiP the frame carries the floating controls and resize zones
          around the panel, so the app never sees them. */}
      <div
        className={cn(isPip && 'group/pip pointer-events-none fixed z-[900]')}
        style={pipFrameStyle}
      >
        {isPip && renderPipControls()}
        {isPip && renderPipResizeZones()}
        <div
          ref={containerRef}
          className={cn(containerClasses, isFillsViewport && 'flex flex-col')}
          style={containerStyle}
        >
          {isFullscreen && renderFullscreenHeader()}
          <div
            ref={contentRef}
            className={cn(
              'relative w-full',
              !isPip && 'flex-1 min-h-0',
              isPip && 'max-h-full overflow-y-auto overflow-x-hidden'
            )}
          >
            {!isPip && renderDisplayModeControls()}
            {renderContent()}
          </div>
        </div>
      </div>
    </>
  );
}
