/**
 * useDisplayMode — Manages display mode state for MCP App containers.
 *
 * Encapsulates the display mode state machine, capability negotiation,
 * PiP drag handling, entrance animations, and postMessage interception
 * for ui/initialize and ui/request-display-mode.
 */

import type { McpUiDisplayMode } from '@modelcontextprotocol/ext-apps/app-bridge';
import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { GooseDisplayMode, OnDisplayModeChange } from './types';

const DEFAULT_IFRAME_HEIGHT = 200;

const AVAILABLE_DISPLAY_MODES: McpUiDisplayMode[] = ['inline', 'fullscreen', 'pip'];

const PIP_WIDTH = 400;
const PIP_HEIGHT = 300;
const PIP_MARGIN_RIGHT = 16;
// Keeps the PiP window above the chat input area (~120px) plus padding.
const PIP_MARGIN_BOTTOM = 140;

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

  /** PiP position offset from the default bottom-right corner. */
  pipPosition: { x: number; y: number };

  /** PiP drag handle event handlers. */
  pipHandlers: {
    onPointerDown: (e: React.PointerEvent) => void;
    onPointerMove: (e: React.PointerEvent) => void;
    onPointerUp: (e: React.PointerEvent) => void;
    onLostPointerCapture: () => void;
    onKeyDown: (e: React.KeyboardEvent) => void;
  };

  /** Ref for the fullscreen close button (auto-focused on enter). */
  fullscreenCloseRef: React.RefObject<HTMLButtonElement | null>;
}

export { AVAILABLE_DISPLAY_MODES, PIP_WIDTH, PIP_HEIGHT, PIP_MARGIN_RIGHT, PIP_MARGIN_BOTTOM };

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

  // ── PiP drag ──────────────────────────────────────────────────────────

  const [pipPosition, setPipPosition] = useState({ x: 0, y: 0 });
  const pipPositionRef = useRef(pipPosition);
  const pipDragRef = useRef<{
    startX: number;
    startY: number;
    originX: number;
    originY: number;
  } | null>(null);

  useEffect(() => {
    pipPositionRef.current = pipPosition;
  }, [pipPosition]);

  const clampPipPosition = useCallback((pos: { x: number; y: number }) => {
    const minX = PIP_WIDTH + PIP_MARGIN_RIGHT - window.innerWidth;
    const maxX = PIP_MARGIN_RIGHT;
    const minY = PIP_HEIGHT + PIP_MARGIN_BOTTOM - window.innerHeight;
    const maxY = PIP_MARGIN_BOTTOM;
    return {
      x: minX > maxX ? 0 : Math.max(minX, Math.min(maxX, pos.x)),
      y: minY > maxY ? 0 : Math.max(minY, Math.min(maxY, pos.y)),
    };
  }, []);

  const handlePipPointerDown = useCallback((e: React.PointerEvent) => {
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    const { x, y } = pipPositionRef.current;
    pipDragRef.current = { startX: e.clientX, startY: e.clientY, originX: x, originY: y };
  }, []);

  const handlePipPointerMove = useCallback(
    (e: React.PointerEvent) => {
      if (!pipDragRef.current) return;
      const dx = e.clientX - pipDragRef.current.startX;
      const dy = e.clientY - pipDragRef.current.startY;
      setPipPosition(
        clampPipPosition({
          x: pipDragRef.current.originX + dx,
          y: pipDragRef.current.originY + dy,
        })
      );
    },
    [clampPipPosition]
  );

  const handlePipPointerUp = useCallback((e: React.PointerEvent) => {
    (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
    pipDragRef.current = null;
  }, []);

  const handlePipLostPointerCapture = useCallback(() => {
    pipDragRef.current = null;
  }, []);

  const handlePipKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      const step = e.shiftKey ? 32 : 8;
      let dx = 0;
      let dy = 0;
      switch (e.key) {
        case 'ArrowUp':
          dy = -step;
          break;
        case 'ArrowDown':
          dy = step;
          break;
        case 'ArrowLeft':
          dx = -step;
          break;
        case 'ArrowRight':
          dx = step;
          break;
        default:
          return;
      }
      e.preventDefault();
      setPipPosition((prev) => clampPipPosition({ x: prev.x + dx, y: prev.y + dy }));
    },
    [clampPipPosition]
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

  // Reset PiP position when entering PiP mode.
  useEffect(() => {
    if (activeDisplayMode === 'pip') {
      setPipPosition({ x: 0, y: 0 });
    }
  }, [activeDisplayMode]);

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
    pipPosition,

    pipHandlers: {
      onPointerDown: handlePipPointerDown,
      onPointerMove: handlePipPointerMove,
      onPointerUp: handlePipPointerUp,
      onLostPointerCapture: handlePipLostPointerCapture,
      onKeyDown: handlePipKeyDown,
    },

    fullscreenCloseRef,
  };
}
