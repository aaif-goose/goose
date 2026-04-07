/**
 * McpAppRenderer — Renders interactive MCP App UIs inside a sandboxed iframe.
 *
 * This component implements the host side of the MCP Apps protocol using the
 * @mcp-ui/client SDK's AppRenderer. It handles resource fetching, sandbox
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

import { AppRenderer, type RequestHandlerExtra } from '@mcp-ui/client';
import type {
  McpUiDisplayMode,
  McpUiHostContext,
  McpUiResourceCsp,
  McpUiResourcePermissions,
  McpUiSizeChangedNotification,
} from '@modelcontextprotocol/ext-apps/app-bridge';
import type { CallToolResult, JSONRPCRequest, Tool } from '@modelcontextprotocol/sdk/types.js';
import { GripHorizontal, Maximize2, PictureInPicture2, X } from 'lucide-react';
import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from 'react';
import { callTool, readResource } from '../../api';
import { getCachedTools } from './toolsCache';
import { AppEvents } from '../../constants/events';
import { useTheme } from '../../contexts/ThemeContext';
import { cn } from '../../utils';
import { errorMessage } from '../../utils/conversionUtils';
import { getProtocol, isProtocolSafe } from '../../utils/urlSecurity';
import { defineMessages, useIntl } from '../../i18n';
import FlyingBird from '../FlyingBird';
import { formatExtensionName } from '../settings/extensions/subcomponents/ExtensionList';
import {
  GooseDisplayMode,
  SandboxPermissions,
  McpAppToolCancelled,
  McpAppToolInput,
  McpAppToolInputPartial,
  DimensionLayout,
  OnDisplayModeChange,
  SamplingCreateMessageParams,
  SamplingCreateMessageResponse,
} from './types';
import {
  useDisplayMode,
  AVAILABLE_DISPLAY_MODES,
  PIP_WIDTH,
  PIP_HEIGHT,
  PIP_MARGIN_RIGHT,
  PIP_MARGIN_BOTTOM,
} from './useDisplayMode';

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
    defaultMessage: 'Move Picture-in-Picture window (use arrow keys)',
  },
  playingInPip: {
    id: 'mcpAppRenderer.playingInPip',
    defaultMessage: 'Playing in Picture-in-Picture',
  },
  invalidUrl: {
    id: 'mcpAppRenderer.invalidUrl',
    defaultMessage: 'Invalid URL',
  },
  openExternalLinkTitle: {
    id: 'mcpAppRenderer.openExternalLinkTitle',
    defaultMessage: 'Open External Link',
  },
  openProtocolLink: {
    id: 'mcpAppRenderer.openProtocolLink',
    defaultMessage: 'Open {protocol} link?',
  },
  openLinkDetail: {
    id: 'mcpAppRenderer.openLinkDetail',
    defaultMessage: 'This will open: {url}',
  },
  cancelButton: {
    id: 'mcpAppRenderer.cancelButton',
    defaultMessage: 'Cancel',
  },
  openButton: {
    id: 'mcpAppRenderer.openButton',
    defaultMessage: 'Open',
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

const DISPLAY_MODE_LAYOUTS: Record<GooseDisplayMode, DimensionLayout> = {
  inline: { width: 'fixed', height: 'unbounded' },
  fullscreen: { width: 'fixed', height: 'fixed' },
  standalone: { width: 'fixed', height: 'fixed' },
  pip: { width: 'fixed', height: 'fixed' },
  // sidecar: { width: 'fixed', height: 'flexible' }, // example on how to use flexible layout
};

function getContainerDimensions(
  displayMode: GooseDisplayMode,
  measuredWidth: number,
  measuredHeight: number
): McpUiHostContext['containerDimensions'] {
  const layout = DISPLAY_MODE_LAYOUTS[displayMode] ?? DISPLAY_MODE_LAYOUTS.inline;

  // Only require a measurement for axes that are fixed or flexible (unbounded axes are omitted).
  if (
    (layout.width !== 'unbounded' && measuredWidth <= 0) ||
    (layout.height !== 'unbounded' && measuredHeight <= 0)
  )
    return undefined;

  const widthDimension = (() => {
    switch (layout.width) {
      case 'fixed':
        return { width: measuredWidth };
      case 'flexible':
        return { maxWidth: measuredWidth };
      case 'unbounded':
        return {};
    }
  })();

  const heightDimension = (() => {
    switch (layout.height) {
      case 'fixed':
        return { height: measuredHeight };
      case 'flexible':
        return { maxHeight: measuredHeight };
      case 'unbounded':
        return {};
    }
  })();

  return { ...widthDimension, ...heightDimension };
}

async function fetchMcpAppProxyUrl(csp: McpUiResourceCsp | null): Promise<string | null> {
  try {
    const baseUrl = await window.electron.getGoosedHostPort();
    const secretKey = await window.electron.getSecretKey();

    if (!baseUrl || !secretKey) {
      console.error('[McpAppRenderer] Failed to get goosed host/port or secret key');
      return null;
    }

    const params = new URLSearchParams();
    params.set('secret', secretKey);

    if (csp?.connectDomains?.length) {
      params.set('connect_domains', csp.connectDomains.join(','));
    }
    if (csp?.resourceDomains?.length) {
      params.set('resource_domains', csp.resourceDomains.join(','));
    }
    if (csp?.frameDomains?.length) {
      params.set('frame_domains', csp.frameDomains.join(','));
    }
    if (csp?.baseUriDomains?.length) {
      params.set('base_uri_domains', csp.baseUriDomains.join(','));
    }

    return `${baseUrl}/mcp-app-proxy?${params.toString()}`;
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

  const dm = useDisplayMode({ displayMode, onDisplayModeChange, containerRef });
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
    pipPosition,
    pipHandlers,
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
          inputSchema: (match.input_schema as Tool['inputSchema']) ?? { type: 'object' as const },
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
  const [apiHost, setApiHost] = useState<string | null>(null);
  const [secretKey, setSecretKey] = useState<string | null>(null);

  useEffect(() => {
    window.electron.getGoosedHostPort().then(setApiHost);
    window.electron.getSecretKey().then(setSecretKey);
  }, []);

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
          const response = await readResource({
            body: {
              session_id: sessionId,
              uri: resourceUri,
              extension_name: extensionName,
            },
          });

          if (cancelled) return;

          if (response.data) {
            const content = response.data;
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

  const handleOpenLink = useCallback(async ({ url }: { url: string }) => {
    if (isProtocolSafe(url)) {
      await window.electron.openExternal(url);
      return { status: 'success' as const };
    }

    const protocol = getProtocol(url);
    if (!protocol) {
      return { status: 'error' as const, message: intl.formatMessage(i18n.invalidUrl) };
    }

    const result = await window.electron.showMessageBox({
      type: 'question',
      buttons: [intl.formatMessage(i18n.cancelButton), intl.formatMessage(i18n.openButton)],
      defaultId: 0,
      title: intl.formatMessage(i18n.openExternalLinkTitle),
      message: intl.formatMessage(i18n.openProtocolLink, { protocol }),
      detail: intl.formatMessage(i18n.openLinkDetail, { url }),
    });

    if (result.response !== 1) {
      return { status: 'error' as const, message: 'User cancelled' };
    }

    await window.electron.openExternal(url);
    return { status: 'success' as const };
  }, [intl]);

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

      const fullToolName = `${extensionName}__${name}`;
      const response = await callTool({
        body: {
          session_id: sessionId,
          name: fullToolName,
          arguments: args || {},
        },
      });

      return {
        content: (response.data?.content || []) as unknown as CallToolResult['content'],
        isError: response.data?.isError || false,
        structuredContent: response.data?.structuredContent as
          | { [key: string]: unknown }
          | undefined,
        _meta: response.data?._meta as { [key: string]: unknown } | undefined,
      };
    },
    [sessionId, extensionName]
  );

  const handleReadResource = useCallback(
    async ({ uri }: { uri: string }) => {
      if (!sessionId) {
        throw new Error('Session not initialized for MCP request');
      }
      const response = await readResource({
        body: {
          session_id: sessionId,
          uri,
          extension_name: extensionName,
        },
      });
      const data = response.data;
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

  // Track the container's pixel dimensions so we can report them to apps via containerDimensions.
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;

    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const { width, height } = entry.contentRect;
        setContainerWidth((prev) => (prev !== Math.round(width) ? Math.round(width) : prev));
        setContainerHeight((prev) => (prev !== Math.round(height) ? Math.round(height) : prev));
      }
    });

    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const handleFallbackRequest = useCallback(
    async (request: JSONRPCRequest, _extra: RequestHandlerExtra) => {
      if (request.method === 'sampling/createMessage') {
        if (!sessionId || !apiHost || !secretKey) {
          throw new Error('Session not initialized for sampling request');
        }
        const { messages, systemPrompt, maxTokens } =
          request.params as unknown as SamplingCreateMessageParams;
        const response = await fetch(`${apiHost}/sessions/${sessionId}/sampling/message`, {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            'X-Secret-Key': secretKey,
          },
          body: JSON.stringify({
            messages: messages.map((m) => ({
              role: m.role,
              content: m.content,
            })),
            systemPrompt,
            maxTokens,
          }),
        });
        if (!response.ok) {
          throw new Error(`Sampling request failed: ${response.statusText}`);
        }
        return (await response.json()) as SamplingCreateMessageResponse;
      }
      return {
        status: 'error' as const,
        message: `Unhandled JSON-RPC method: ${request.method ?? '<unknown>'}`,
      };
    },
    [sessionId, apiHost, secretKey]
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
      permissions: meta.permissions || 'allow-scripts allow-same-origin',
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

  const isToolCancelled = !!toolCancelled;
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
      <AppRenderer
        sandbox={sandboxConfig}
        toolName={resourceUri}
        html={html ?? undefined}
        toolInput={toolInput?.arguments}
        toolInputPartial={toolInputPartial ? { arguments: toolInputPartial.arguments } : undefined}
        toolCancelled={isToolCancelled}
        hostContext={hostContext}
        toolResult={toolResult}
        onOpenLink={handleOpenLink}
        onMessage={handleMessage}
        onCallTool={handleCallTool}
        onReadResource={handleReadResource}
        onLoggingMessage={handleLoggingMessage}
        onSizeChanged={handleSizeChanged}
        onFallbackRequest={handleFallbackRequest}
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

    if (activeDisplayMode === 'pip') {
      return (
        <>
          {appSupportsFullscreen && (
            <button
              onClick={() => changeDisplayMode('fullscreen')}
              className="cursor-pointer rounded-md bg-black/50 p-1 text-white backdrop-blur-sm transition-opacity hover:bg-black/70"
              title={intl.formatMessage(i18n.fullscreen)}
              aria-label={intl.formatMessage(i18n.fullscreen)}
            >
              <Maximize2 size={14} />
            </button>
          )}
          <button
            onClick={() => changeDisplayMode('inline')}
            className="cursor-pointer rounded-md bg-black/50 p-1 text-white backdrop-blur-sm transition-opacity hover:bg-black/70"
            title={intl.formatMessage(i18n.close)}
            aria-label={intl.formatMessage(i18n.close)}
          >
            <X size={14} />
          </button>
        </>
      );
    }

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
  // The AppRenderer and its iframe are never unmounted, preserving app state across mode changes.
  const containerClasses = cn(
    'mcp-app-container bg-background-primary [&_iframe]:!w-full',
    isFillsViewport && 'fixed inset-0 z-[1000] overflow-hidden [&_iframe]:!h-full',
    isPip &&
      'fixed z-[900] overflow-y-auto overflow-x-hidden rounded-xl border border-border-primary shadow-2xl',
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
            width: `${PIP_WIDTH}px`,
            height: `${PIP_HEIGHT}px`,
            right: `${PIP_MARGIN_RIGHT - pipPosition.x}px`,
            bottom: `${PIP_MARGIN_BOTTOM - pipPosition.y}px`,
          }
        : {
            width: '100%',
            height: `${effectiveInlineHeight}px`,
          }),
  };

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

      {/* Stable app container — never unmounted, only repositioned via CSS */}
      <div
        ref={containerRef}
        className={cn(containerClasses, isFillsViewport && 'flex flex-col', isPip && 'group/pip')}
        style={containerStyle}
      >
        {isFullscreen && renderFullscreenHeader()}
        {isPip && (
          <div className="pointer-events-none sticky top-1 z-20 flex h-0 items-start justify-between px-1 opacity-0 transition-opacity group-hover/pip:pointer-events-auto group-hover/pip:opacity-100 focus-within:pointer-events-auto focus-within:opacity-100">
            <div
              role="button"
              tabIndex={0}
              aria-label={intl.formatMessage(i18n.movePipWindow)}
              className="pointer-events-auto cursor-grab rounded-md bg-black/50 p-1 text-white backdrop-blur-sm hover:bg-black/70 active:cursor-grabbing"
              onPointerDown={pipHandlers.onPointerDown}
              onPointerMove={pipHandlers.onPointerMove}
              onPointerUp={pipHandlers.onPointerUp}
              onLostPointerCapture={pipHandlers.onLostPointerCapture}
              onKeyDown={pipHandlers.onKeyDown}
            >
              <GripHorizontal size={14} />
            </div>
            <div className="flex gap-1">{renderDisplayModeControls()}</div>
          </div>
        )}
        <div ref={contentRef} className={cn('relative w-full', !isPip && 'flex-1 min-h-0')}>
          {!isPip && renderDisplayModeControls()}
          {renderContent()}
        </div>
      </div>
    </>
  );
}
