/**
 * Picture-in-Picture display mode for MCP Apps: geometry, pointer and keyboard
 * gestures, per-session size memory, and the chrome drawn around the app.
 *
 * The app's stable container lives in McpAppRenderer and is only restyled when
 * the mode changes, so the iframe is never remounted. This module supplies the
 * classes and styles that container takes on in PiP and renders the title bar
 * and resize zones as siblings of it inside the PiP frame.
 *
 * The panel is anchored to the bottom-right corner of the main window. `x`/`y`
 * are offsets from the default anchored position (positive x moves right,
 * positive y moves down), so a panel that has never been moved stays glued to
 * the corner when the main window is resized. The panel sits inside a slightly
 * larger transparent frame that holds the title bar above it and the resize
 * zones straddling its edges. Clamping keeps the whole frame inside the viewport.
 */

import { Maximize2, PictureInPicture2, X } from 'lucide-react';
import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { defineMessages, useIntl } from '../../i18n';
import { cn } from '../../utils';

const i18n = defineMessages({
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
});

// ── Geometry ────────────────────────────────────────────────────────────

export const PIP_MIN_WIDTH = 400;
export const PIP_MIN_HEIGHT = 300;
export const PIP_MARGIN_RIGHT = 16;
// Keeps the PiP window above the chat input area (~120px) plus padding.
export const PIP_MARGIN_BOTTOM = 140;

/** Square hit zone centered on each window corner. */
export const PIP_CORNER_ZONE = 24;
/** Thickness of the strip centered on each window edge. */
export const PIP_EDGE_ZONE = 8;
/** Title bar shown above the panel while the window is hovered. */
export const PIP_TITLE_BAR_HEIGHT = 32;
// Matches the panel's rounded-xl radius so the title bar fills the corner gap.
const PIP_TITLE_BAR_OVERLAP = 12;
/**
 * Space between the panel and the frame edges. The top holds the title bar
 * plus room for the resize zones that straddle the bar's top edge.
 */
export const PIP_FRAME_INSET = {
  top: PIP_TITLE_BAR_HEIGHT + PIP_CORNER_ZONE / 2,
  right: 12,
  bottom: 12,
  left: 12,
} as const;

export interface PipGeometry {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Viewport {
  width: number;
  height: number;
}

export interface Rect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export const DEFAULT_PIP_GEOMETRY: PipGeometry = {
  x: 0,
  y: 0,
  width: PIP_MIN_WIDTH,
  height: PIP_MIN_HEIGHT,
};

export type PipCorner = 'top-left' | 'top-right' | 'bottom-left' | 'bottom-right';
export type PipEdge = 'top' | 'right' | 'bottom' | 'left';
export type PipResizeHandle = PipCorner | PipEdge;

export const PIP_CORNERS: readonly PipCorner[] = [
  'top-left',
  'top-right',
  'bottom-left',
  'bottom-right',
];
export const PIP_EDGES: readonly PipEdge[] = ['top', 'right', 'bottom', 'left'];
export const PIP_RESIZE_HANDLES: readonly PipResizeHandle[] = [...PIP_EDGES, ...PIP_CORNERS];

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

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

export function getViewport(): Viewport {
  return { width: window.innerWidth, height: window.innerHeight };
}

/** Top-left corner of the PiP panel in viewport coordinates. */
export function pipTopLeft(
  geometry: PipGeometry,
  viewport: Viewport
): { left: number; top: number } {
  return {
    left: viewport.width - PIP_MARGIN_RIGHT + geometry.x - geometry.width,
    top: viewport.height - PIP_MARGIN_BOTTOM + geometry.y - geometry.height,
  };
}

/** Frame size and its offset from the viewport's bottom-right corner. */
export function pipFrameRect(geometry: PipGeometry): {
  width: number;
  height: number;
  right: number;
  bottom: number;
} {
  const { top, right, bottom, left } = PIP_FRAME_INSET;
  return {
    width: geometry.width + left + right,
    height: geometry.height + top + bottom,
    right: PIP_MARGIN_RIGHT - geometry.x - right,
    bottom: PIP_MARGIN_BOTTOM - geometry.y - bottom,
  };
}

/**
 * Hit zone for a resize handle, in frame coordinates. The window being resized
 * is the title bar plus the panel, so the top zones sit on the bar's top edge
 * rather than on the seam between the bar and the panel.
 */
export function pipResizeZoneRect(handle: PipResizeHandle, geometry: PipGeometry): Rect {
  const half = PIP_CORNER_ZONE / 2;
  const edgeHalf = PIP_EDGE_ZONE / 2;
  const left = PIP_FRAME_INSET.left;
  const top = PIP_FRAME_INSET.top - PIP_TITLE_BAR_HEIGHT;
  const right = left + geometry.width;
  const bottom = PIP_FRAME_INSET.top + geometry.height;
  const windowHeight = bottom - top;

  switch (handle) {
    case 'top-left':
      return {
        left: left - half,
        top: top - half,
        width: PIP_CORNER_ZONE,
        height: PIP_CORNER_ZONE,
      };
    case 'top-right':
      return {
        left: right - half,
        top: top - half,
        width: PIP_CORNER_ZONE,
        height: PIP_CORNER_ZONE,
      };
    case 'bottom-left':
      return {
        left: left - half,
        top: bottom - half,
        width: PIP_CORNER_ZONE,
        height: PIP_CORNER_ZONE,
      };
    case 'bottom-right':
      return {
        left: right - half,
        top: bottom - half,
        width: PIP_CORNER_ZONE,
        height: PIP_CORNER_ZONE,
      };
    case 'top':
      return {
        left: left + half,
        top: top - edgeHalf,
        width: geometry.width - PIP_CORNER_ZONE,
        height: PIP_EDGE_ZONE,
      };
    case 'bottom':
      return {
        left: left + half,
        top: bottom - edgeHalf,
        width: geometry.width - PIP_CORNER_ZONE,
        height: PIP_EDGE_ZONE,
      };
    case 'left':
      return {
        left: left - edgeHalf,
        top: top + half,
        width: PIP_EDGE_ZONE,
        height: windowHeight - PIP_CORNER_ZONE,
      };
    case 'right':
      return {
        left: right - edgeHalf,
        top: top + half,
        width: PIP_EDGE_ZONE,
        height: windowHeight - PIP_CORNER_ZONE,
      };
  }
}

/**
 * Clamps size to [minimum, viewport minus frame inset] and position so the
 * frame stays fully inside the viewport. When the viewport is too small on an
 * axis, that axis falls back to the default anchored offset.
 */
export function clampPipGeometry(geometry: PipGeometry, viewport: Viewport): PipGeometry {
  const inset = PIP_FRAME_INSET;
  const width = clamp(
    geometry.width,
    PIP_MIN_WIDTH,
    Math.max(PIP_MIN_WIDTH, viewport.width - inset.left - inset.right)
  );
  const height = clamp(
    geometry.height,
    PIP_MIN_HEIGHT,
    Math.max(PIP_MIN_HEIGHT, viewport.height - inset.top - inset.bottom)
  );

  const minX = width + inset.left + PIP_MARGIN_RIGHT - viewport.width;
  const maxX = PIP_MARGIN_RIGHT - inset.right;
  const minY = height + inset.top + PIP_MARGIN_BOTTOM - viewport.height;
  const maxY = PIP_MARGIN_BOTTOM - inset.bottom;

  return {
    width,
    height,
    x: minX > maxX ? 0 : clamp(geometry.x, minX, maxX),
    y: minY > maxY ? 0 : clamp(geometry.y, minY, maxY),
  };
}

export function movePipGeometry(
  origin: PipGeometry,
  dx: number,
  dy: number,
  viewport: Viewport
): PipGeometry {
  return clampPipGeometry({ ...origin, x: origin.x + dx, y: origin.y + dy }, viewport);
}

/**
 * Resizes by dragging `handle`, keeping the opposite edge(s) fixed and never
 * pushing the frame past the viewport edges.
 */
export function resizePipGeometry(
  origin: PipGeometry,
  dx: number,
  dy: number,
  viewport: Viewport,
  handle: PipResizeHandle
): PipGeometry {
  const inset = PIP_FRAME_INSET;
  const { left, top } = pipTopLeft(origin, viewport);
  const fromLeft = handle.includes('left');
  const fromRight = handle.includes('right');
  const fromTop = handle.includes('top');
  const fromBottom = handle.includes('bottom');

  let width = origin.width;
  if (fromLeft || fromRight) {
    const maxWidth = Math.max(
      PIP_MIN_WIDTH,
      fromLeft ? left + origin.width - inset.left : viewport.width - inset.right - left
    );
    width = clamp(origin.width + (fromLeft ? -dx : dx), PIP_MIN_WIDTH, maxWidth);
  }

  let height = origin.height;
  if (fromTop || fromBottom) {
    const maxHeight = Math.max(
      PIP_MIN_HEIGHT,
      fromTop ? top + origin.height - inset.top : viewport.height - inset.bottom - top
    );
    height = clamp(origin.height + (fromTop ? -dy : dy), PIP_MIN_HEIGHT, maxHeight);
  }

  // x/y track the right/bottom edges, so they only change when those edges move.
  return clampPipGeometry(
    {
      width,
      height,
      x: fromRight ? origin.x + (width - origin.width) : origin.x,
      y: fromBottom ? origin.y + (height - origin.height) : origin.y,
    },
    viewport
  );
}

// ── Per-session memory ──────────────────────────────────────────────────

/**
 * Remembers the most recent PiP geometry per chat session for the lifetime of
 * the app process. New sessions start from the defaults.
 */
const sessionGeometry = new Map<string, PipGeometry>();

export function loadPipGeometry(sessionId: string | null | undefined): PipGeometry | undefined {
  return sessionId ? sessionGeometry.get(sessionId) : undefined;
}

export function savePipGeometry(sessionId: string | null | undefined, geometry: PipGeometry) {
  if (sessionId) sessionGeometry.set(sessionId, geometry);
}

export function clearPipGeometryStore() {
  sessionGeometry.clear();
}

// ── Gestures ────────────────────────────────────────────────────────────

export interface PipGestureHandlers {
  onPointerDown: (e: React.PointerEvent) => void;
  onPointerMove: (e: React.PointerEvent) => void;
  onPointerUp: (e: React.PointerEvent) => void;
  onLostPointerCapture: () => void;
  onKeyDown: (e: React.KeyboardEvent) => void;
}

type PipGestureApply = (
  origin: PipGeometry,
  dx: number,
  dy: number,
  viewport: Viewport
) => PipGeometry;

/**
 * Pointer-drag and arrow-key gesture that applies a (dx, dy) delta to the PiP
 * geometry. Used for both moving (delta shifts position) and resizing (delta
 * moves the dragged edge or corner).
 */
function createPipGesture(
  geometryRef: React.RefObject<PipGeometry>,
  setGeometry: (next: PipGeometry) => void,
  apply: PipGestureApply
): PipGestureHandlers {
  let drag: { startX: number; startY: number; origin: PipGeometry } | null = null;

  return {
    onPointerDown: (e) => {
      e.preventDefault();
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
      drag = { startX: e.clientX, startY: e.clientY, origin: geometryRef.current };
    },
    onPointerMove: (e) => {
      if (!drag) return;
      setGeometry(
        apply(drag.origin, e.clientX - drag.startX, e.clientY - drag.startY, getViewport())
      );
    },
    onPointerUp: (e) => {
      (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
      drag = null;
    },
    onLostPointerCapture: () => {
      drag = null;
    },
    onKeyDown: (e) => {
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
      setGeometry(apply(geometryRef.current, dx, dy, getViewport()));
    },
  };
}

// ── Hook ────────────────────────────────────────────────────────────────

export interface PipWindowState {
  /** Size and position offset from the default bottom-right corner. */
  geometry: PipGeometry;
  moveHandlers: PipGestureHandlers;
  /** One set of handlers per edge and corner. */
  resizeHandlers: Record<PipResizeHandle, PipGestureHandlers>;
}

interface UsePipWindowOptions {
  /** Whether the app is currently displayed in PiP. */
  active: boolean;
  /** Chat session used to remember size and position between openings. */
  sessionId?: string | null;
}

export function usePipWindow({ active, sessionId }: UsePipWindowOptions): PipWindowState {
  const [geometry, setGeometry] = useState<PipGeometry>(DEFAULT_PIP_GEOMETRY);
  const geometryRef = useRef(geometry);

  const updateGeometry = useCallback(
    (next: PipGeometry) => {
      geometryRef.current = next;
      setGeometry(next);
      savePipGeometry(sessionId, next);
    },
    [sessionId]
  );

  const moveHandlers = useMemo(
    () => createPipGesture(geometryRef, updateGeometry, movePipGeometry),
    [updateGeometry]
  );
  const resizeHandlers = useMemo(
    () =>
      Object.fromEntries(
        PIP_RESIZE_HANDLES.map((handle) => [
          handle,
          createPipGesture(geometryRef, updateGeometry, (origin, dx, dy, viewport) =>
            resizePipGeometry(origin, dx, dy, viewport, handle)
          ),
        ])
      ) as Record<PipResizeHandle, PipGestureHandlers>,
    [updateGeometry]
  );

  // Entering PiP restores the session's last geometry (or defaults), re-clamped
  // to the current window so a smaller window never leaves it off-screen.
  useEffect(() => {
    if (!active) return;
    updateGeometry(
      clampPipGeometry(loadPipGeometry(sessionId) ?? DEFAULT_PIP_GEOMETRY, getViewport())
    );
  }, [active, sessionId, updateGeometry]);

  useEffect(() => {
    if (!active) return;
    const handleResize = () => {
      updateGeometry(clampPipGeometry(geometryRef.current, getViewport()));
    };
    window.addEventListener('resize', handleResize);
    return () => window.removeEventListener('resize', handleResize);
  }, [active, updateGeometry]);

  return { geometry, moveHandlers, resizeHandlers };
}

// ── Shell styling ───────────────────────────────────────────────────────

/**
 * Classes the stable app shell takes on in PiP. The frame wraps the panel and
 * carries the hover group for the title bar; the panel is the app container;
 * the content element scrolls the app.
 */
export const PIP_SHELL_CLASSES = {
  frame: 'group/pip pointer-events-none fixed z-[900]',
  panel:
    'pointer-events-auto absolute z-10 overflow-hidden rounded-xl border border-border-primary shadow-2xl',
  content: 'max-h-full overflow-y-auto overflow-x-hidden',
} as const;

export function pipFrameStyle(geometry: PipGeometry): React.CSSProperties {
  const frame = pipFrameRect(geometry);
  return {
    width: `${frame.width}px`,
    height: `${frame.height}px`,
    right: `${frame.right}px`,
    bottom: `${frame.bottom}px`,
  };
}

export function pipPanelStyle(geometry: PipGeometry): React.CSSProperties {
  return {
    top: `${PIP_FRAME_INSET.top}px`,
    left: `${PIP_FRAME_INSET.left}px`,
    width: `${geometry.width}px`,
    height: `${geometry.height}px`,
  };
}

// ── Chrome ──────────────────────────────────────────────────────────────

interface PipWindowProps extends PipWindowState {
  title: string;
  /** Omitted when the app does not support fullscreen. */
  onFullscreen?: () => void;
  onClose: () => void;
}

/**
 * Title bar and resize zones rendered inside the PiP frame as siblings of the
 * app panel, so the app never sees them and the panel is never remounted.
 *
 * The title bar is styled like the fullscreen header. A drag layer fills the
 * bar so the whole bar moves the window, with the buttons layered over it. The
 * bar only takes pointer events while the PiP is hovered or focused, so it
 * never intercepts clicks meant for the chat behind it. It extends under the
 * panel's rounded top corners so the two join cleanly.
 */
export function PipWindow({
  geometry,
  moveHandlers,
  resizeHandlers,
  title,
  onFullscreen,
  onClose,
}: PipWindowProps) {
  const intl = useIntl();

  return (
    <>
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
          onPointerDown={moveHandlers.onPointerDown}
          onPointerMove={moveHandlers.onPointerMove}
          onPointerUp={moveHandlers.onPointerUp}
          onLostPointerCapture={moveHandlers.onLostPointerCapture}
          onKeyDown={moveHandlers.onKeyDown}
        />
        <div className="pointer-events-none min-w-0 flex-1" />
        <span className="pointer-events-none truncate px-3 text-xs font-medium text-text-secondary">
          {title}
        </span>
        <div className="pointer-events-none relative flex flex-1 items-center justify-end gap-0.5">
          {onFullscreen && (
            <button
              onClick={onFullscreen}
              className="pointer-events-auto cursor-pointer rounded-md p-1 text-text-secondary transition-colors hover:bg-black/10 hover:text-text-primary dark:hover:bg-white/10"
              title={intl.formatMessage(i18n.fullscreen)}
              aria-label={intl.formatMessage(i18n.fullscreen)}
            >
              <Maximize2 size={14} />
            </button>
          )}
          <button
            onClick={onClose}
            className="pointer-events-auto cursor-pointer rounded-md p-1 text-text-secondary transition-colors hover:bg-black/10 hover:text-text-primary dark:hover:bg-white/10"
            title={intl.formatMessage(i18n.close)}
            aria-label={intl.formatMessage(i18n.close)}
          >
            <X size={14} />
          </button>
        </div>
      </div>

      {PIP_RESIZE_HANDLES.map((handle) => {
        const handlers = resizeHandlers[handle];
        const zone = pipResizeZoneRect(handle, geometry);
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
      })}
    </>
  );
}

/** Stands in for the app in the chat flow while it is detached into PiP. */
export function PipPlaceholder({ height, onReturn }: { height: number; onReturn: () => void }) {
  const intl = useIntl();

  return (
    <div
      className="mt-6 mb-2 flex items-center justify-center rounded-lg border border-dashed border-border-primary bg-black/[0.02] dark:bg-white/[0.02]"
      style={{ width: '100%', height: `${height}px` }}
    >
      <button
        onClick={onReturn}
        className="cursor-pointer flex items-center gap-2 rounded-md px-3 py-1.5 text-xs text-text-secondary transition-colors hover:bg-black/5 hover:text-text-primary dark:hover:bg-white/5"
      >
        <PictureInPicture2 size={14} />
        <span>{intl.formatMessage(i18n.playingInPip)}</span>
      </button>
    </div>
  );
}
