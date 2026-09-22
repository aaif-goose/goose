/**
 * Geometry helpers for the MCP App Picture-in-Picture window.
 *
 * The PiP panel is anchored to the bottom-right corner of the main window.
 * `x`/`y` are offsets from the default anchored position (positive x moves
 * right, positive y moves down), so a panel that has never been moved stays
 * glued to the bottom-right corner when the main window is resized.
 *
 * The panel sits inside a slightly larger transparent frame that holds the
 * floating control row above it and the resize zones straddling its edges.
 * Clamping keeps the whole frame inside the viewport.
 */

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
