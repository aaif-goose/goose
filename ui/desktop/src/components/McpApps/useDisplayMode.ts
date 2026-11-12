/**
 * useDisplayMode — Manages display mode state for MCP App containers.
 *
 * Encapsulates the display mode state machine, capability negotiation,
 * entrance animations, and postMessage interception for ui/initialize and
 * ui/request-display-mode. Mode-specific chrome and geometry live with each
 * mode's component (see PipWindow.tsx).
 */

import type { McpUiDisplayMode } from '@modelcontextprotocol/ext-apps/app-bridge';
import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { GooseDisplayMode, OnDisplayModeChange } from './types';

const DEFAULT_IFRAME_HEIGHT = 200;

const AVAILABLE_DISPLAY_MODES: McpUiDisplayMode[] = ['inline', 'fullscreen', 'pip'];

interface UseDisplayModeOptions {
  displayMode: GooseDisplayMode;
  onDisplayModeChange?: OnDisplayModeChange;
  containerRef: React.RefObject<HTMLDivElement | null>;
}

export interface DisplayModeState {
  activeDisplayMode: GooseDisplayMode;
  effectiveDisplayModes: McpUiDisplayMode[];
  isStandalone: boolean;
  isFullscreen: boolean;
  isPip: boolean;
  isFillsViewport: boolean;
  isInline: boolean;
  appSupportsFullscreen: boolean;
  appSupportsPip: boolean;
  appTitle: string | null;

  changeDisplayMode: (mode: GooseDisplayMode) => void;

  /** Remembered inline height for placeholders when detached. */
  inlineHeight: number;

  /** Ref for the fullscreen close button (auto-focused on enter). */
  fullscreenCloseRef: React.RefObject<HTMLButtonElement | null>;
}

export { AVAILABLE_DISPLAY_MODES };

export function useDisplayMode({
  displayMode,
  onDisplayModeChange,
  containerRef,
}: UseDisplayModeOptions): DisplayModeState {
  const [activeDisplayMode, setActiveDisplayMode] = useState<GooseDisplayMode>(displayMode);

  useEffect(() => {
    setActiveDisplayMode(displayMode);
  }, [displayMode]);

  const isStandalone = displayMode === 'standalone';

  // Display modes the app declared support for during ui/initialize.
  // null = not yet known (controls stay hidden until initialize), empty = app didn't declare any.
  const [appDeclaredModes, setAppDeclaredModes] = useState<string[] | null>(null);

  // App-declared title from ui/initialize (highest priority in the title fallback chain).
  const [appTitle, setAppTitle] = useState<string | null>(null);

  const effectiveDisplayModes = useMemo((): McpUiDisplayMode[] => {
    if (!appDeclaredModes) return [];
    return AVAILABLE_DISPLAY_MODES.filter((m) => appDeclaredModes.includes(m));
  }, [appDeclaredModes]);

  // Snapshot of the container height captured when leaving inline mode.
  // Stored as state (not a ref) so consumers re-render with the correct value
  // for placeholders and for restoring the inline container on return.
  const [savedInlineHeight, setSavedInlineHeight] = useState(DEFAULT_IFRAME_HEIGHT);

  // Cache iframe contentWindows for O(1) message source matching.
  // eslint-disable-next-line no-undef
  const iframeWindowsRef = useRef<Set<Window>>(new Set());

  const enterAnimRef = useRef<string | null>(null);
  const fullscreenCloseRef = useRef<HTMLButtonElement>(null);

  // ── Mode transitions ──────────────────────────────────────────────────

  const changeDisplayMode = useCallback(
    (mode: GooseDisplayMode) => {
      const el = containerRef.current;
      const prefersReducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

      if (activeDisplayMode === 'inline' && el) {
        setSavedInlineHeight(el.getBoundingClientRect().height || DEFAULT_IFRAME_HEIGHT);
      }

      if (enterAnimRef.current && el) {
        el.classList.remove(enterAnimRef.current);
        enterAnimRef.current = null;
      }

      setActiveDisplayMode(mode);
      onDisplayModeChange?.(mode);

      if (el && !prefersReducedMotion && mode !== activeDisplayMode) {
        const animClass =
          mode === 'pip'
            ? 'mcp-enter-pip'
            : mode === 'fullscreen'
              ? 'mcp-enter-fullscreen'
              : 'mcp-enter-inline';

        requestAnimationFrame(() => {
          el.classList.add(animClass);
          enterAnimRef.current = animClass;

          el.addEventListener(
            'animationend',
            () => {
              el.classList.remove(animClass);
              if (enterAnimRef.current === animClass) {
                enterAnimRef.current = null;
              }
            },
            { once: true }
          );
        });
      }
    },
    [onDisplayModeChange, activeDisplayMode, containerRef]
  );

  // ── Effects ───────────────────────────────────────────────────────────

  // Cache iframe contentWindows for O(1) source matching via MutationObserver.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const refreshCache = () => {
      const windows = iframeWindowsRef.current;
      windows.clear();
      container.querySelectorAll('iframe').forEach((iframe) => {
        if (iframe.contentWindow) windows.add(iframe.contentWindow);
      });
    };

    refreshCache();
    const observer = new MutationObserver(refreshCache);
    observer.observe(container, { childList: true, subtree: true });
    return () => observer.disconnect();
  }, [containerRef]);

  // Intercept app postMessages for:
  // 1. ui/initialize — extract appCapabilities.availableDisplayModes
  // 2. ui/request-display-mode — change display mode on behalf of the app
  useEffect(() => {
    if (isStandalone) return;

    const handleMessage = (e: MessageEvent) => {
      const data = e.data;
      if (!data || typeof data !== 'object') return;
      // eslint-disable-next-line no-undef
      if (!e.source || !iframeWindowsRef.current.has(e.source as Window)) return;

      if (data.method === 'ui/initialize' && data.params) {
        const caps = data.params.appCapabilities || data.params.capabilities;
        if (caps?.availableDisplayModes && Array.isArray(caps.availableDisplayModes)) {
          setAppDeclaredModes(caps.availableDisplayModes);
        }
        const title = data.params.clientInfo?.name;
        if (typeof title === 'string' && title.trim()) {
          setAppTitle(title.trim());
        }
      }

      // After initialize, only allow modes both host and app agree on.
      // Before initialize (effectiveDisplayModes empty), fall back to the full host list.
      if (data.method === 'ui/request-display-mode' && data.params?.mode) {
        const requested = data.params.mode as McpUiDisplayMode;
        const allowed =
          effectiveDisplayModes.length > 0 ? effectiveDisplayModes : AVAILABLE_DISPLAY_MODES;
        if (allowed.includes(requested)) {
          changeDisplayMode(requested);
        }
      }
    };

    window.addEventListener('message', handleMessage);
    return () => window.removeEventListener('message', handleMessage);
  }, [isStandalone, changeDisplayMode, effectiveDisplayModes]);

  // Escape key exits fullscreen.
  useEffect(() => {
    if (activeDisplayMode !== 'fullscreen') return;
    fullscreenCloseRef.current?.focus();
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') changeDisplayMode('inline');
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [activeDisplayMode, changeDisplayMode]);

  // ── Derived state ─────────────────────────────────────────────────────

  const isFullscreen = activeDisplayMode === 'fullscreen';
  const isPip = activeDisplayMode === 'pip';
  const isFillsViewport = isFullscreen || isStandalone;
  const isInline = !isFillsViewport && !isPip;

  const appSupportsFullscreen = effectiveDisplayModes.includes('fullscreen');
  const appSupportsPip = effectiveDisplayModes.includes('pip');

  return {
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

    inlineHeight: savedInlineHeight,

    fullscreenCloseRef,
  };
}
