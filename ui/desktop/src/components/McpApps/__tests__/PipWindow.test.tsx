import type React from 'react';
import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  DEFAULT_PIP_GEOMETRY,
  PIP_CORNERS,
  PIP_EDGES,
  PIP_FRAME_INSET,
  PIP_MARGIN_BOTTOM,
  PIP_MARGIN_RIGHT,
  PIP_MIN_HEIGHT,
  PIP_MIN_WIDTH,
  PIP_TITLE_BAR_HEIGHT,
  clampPipGeometry,
  clearPipGeometryStore,
  movePipGeometry,
  pipResizeZoneRect,
  pipTopLeft,
  resizePipGeometry,
  usePipWindow,
} from '../PipWindow';

const viewport = { width: 1200, height: 900 };

function corners(geometry: typeof DEFAULT_PIP_GEOMETRY) {
  const { left, top } = pipTopLeft(geometry, viewport);
  return { left, top, right: left + geometry.width, bottom: top + geometry.height };
}

describe('clampPipGeometry', () => {
  it('enforces the minimum size', () => {
    const result = clampPipGeometry({ x: 0, y: 0, width: 100, height: 50 }, viewport);
    expect(result.width).toBe(PIP_MIN_WIDTH);
    expect(result.height).toBe(PIP_MIN_HEIGHT);
  });

  it('caps the size at the viewport minus the frame inset', () => {
    const result = clampPipGeometry({ x: 0, y: 0, width: 5000, height: 5000 }, viewport);
    expect(result.width).toBe(viewport.width - PIP_FRAME_INSET.left - PIP_FRAME_INSET.right);
    expect(result.height).toBe(viewport.height - PIP_FRAME_INSET.top - PIP_FRAME_INSET.bottom);
  });

  it('keeps the frame inside the viewport on every edge', () => {
    const farLeftUp = clampPipGeometry({ ...DEFAULT_PIP_GEOMETRY, x: -5000, y: -5000 }, viewport);
    expect(pipTopLeft(farLeftUp, viewport)).toEqual({
      left: PIP_FRAME_INSET.left,
      top: PIP_FRAME_INSET.top,
    });

    const farRightDown = clampPipGeometry({ ...DEFAULT_PIP_GEOMETRY, x: 5000, y: 5000 }, viewport);
    expect(farRightDown.x).toBe(PIP_MARGIN_RIGHT - PIP_FRAME_INSET.right);
    expect(farRightDown.y).toBe(PIP_MARGIN_BOTTOM - PIP_FRAME_INSET.bottom);
  });

  it('falls back to the anchored offset when the viewport is smaller than the window', () => {
    const tiny = { width: 300, height: 200 };
    const result = clampPipGeometry({ x: 50, y: 50, width: 800, height: 600 }, tiny);
    expect(result).toEqual({ x: 0, y: 0, width: PIP_MIN_WIDTH, height: PIP_MIN_HEIGHT });
  });
});

describe('movePipGeometry', () => {
  it('shifts position without changing size and clamps to the viewport', () => {
    expect(movePipGeometry(DEFAULT_PIP_GEOMETRY, -100, -50, viewport)).toEqual({
      ...DEFAULT_PIP_GEOMETRY,
      x: -100,
      y: -50,
    });

    const clamped = movePipGeometry(DEFAULT_PIP_GEOMETRY, 1000, 1000, viewport);
    expect(clamped.x).toBe(PIP_MARGIN_RIGHT - PIP_FRAME_INSET.right);
    expect(clamped.y).toBe(PIP_MARGIN_BOTTOM - PIP_FRAME_INSET.bottom);
  });
});

describe('resizePipGeometry', () => {
  const movedAwayFromCorner = { ...DEFAULT_PIP_GEOMETRY, x: -300, y: -200 };

  it('grows from each corner while keeping the opposite corner fixed', () => {
    const before = corners(movedAwayFromCorner);
    for (const corner of PIP_CORNERS) {
      const dx = corner.endsWith('left') ? -120 : 120;
      const dy = corner.startsWith('top') ? -80 : 80;
      const after = corners(resizePipGeometry(movedAwayFromCorner, dx, dy, viewport, corner));

      expect(after.right - after.left).toBe(PIP_MIN_WIDTH + 120);
      expect(after.bottom - after.top).toBe(PIP_MIN_HEIGHT + 80);
      expect(after[corner.endsWith('left') ? 'right' : 'left']).toBe(
        before[corner.endsWith('left') ? 'right' : 'left']
      );
      expect(after[corner.startsWith('top') ? 'bottom' : 'top']).toBe(
        before[corner.startsWith('top') ? 'bottom' : 'top']
      );
    }
  });

  it('resizes a single axis from each edge, keeping the opposite edge fixed', () => {
    const before = corners(movedAwayFromCorner);

    const top = corners(resizePipGeometry(movedAwayFromCorner, 999, -50, viewport, 'top'));
    expect(top).toEqual({ ...before, top: before.top - 50 });

    const bottom = corners(resizePipGeometry(movedAwayFromCorner, 999, 50, viewport, 'bottom'));
    expect(bottom).toEqual({ ...before, bottom: before.bottom + 50 });

    const left = corners(resizePipGeometry(movedAwayFromCorner, -50, 999, viewport, 'left'));
    expect(left).toEqual({ ...before, left: before.left - 50 });

    const right = corners(resizePipGeometry(movedAwayFromCorner, 50, 999, viewport, 'right'));
    expect(right).toEqual({ ...before, right: before.right + 50 });
  });

  it('never shrinks below the minimum size from any corner', () => {
    for (const corner of PIP_CORNERS) {
      const shrunk = resizePipGeometry(
        movedAwayFromCorner,
        corner.endsWith('left') ? 500 : -500,
        corner.startsWith('top') ? 500 : -500,
        viewport,
        corner
      );
      expect(shrunk.width).toBe(PIP_MIN_WIDTH);
      expect(shrunk.height).toBe(PIP_MIN_HEIGHT);
    }
  });

  it('stops growing at the viewport edges without moving the fixed corner', () => {
    const before = pipTopLeft(DEFAULT_PIP_GEOMETRY, viewport);
    const result = resizePipGeometry(DEFAULT_PIP_GEOMETRY, 5000, 5000, viewport, 'bottom-right');
    const after = pipTopLeft(result, viewport);

    expect(after).toEqual(before);
    expect(after.left + result.width).toBe(viewport.width - PIP_FRAME_INSET.right);
    expect(after.top + result.height).toBe(viewport.height - PIP_FRAME_INSET.bottom);

    const topLeft = corners(
      resizePipGeometry(movedAwayFromCorner, -5000, -5000, viewport, 'top-left')
    );
    expect(topLeft.left).toBe(PIP_FRAME_INSET.left);
    expect(topLeft.top).toBe(PIP_FRAME_INSET.top);
  });
});

describe('pipResizeZoneRect', () => {
  const geometry = DEFAULT_PIP_GEOMETRY;
  // The resizable window is the title bar plus the panel.
  const panel = {
    left: PIP_FRAME_INSET.left,
    top: PIP_FRAME_INSET.top - PIP_TITLE_BAR_HEIGHT,
    right: PIP_FRAME_INSET.left + geometry.width,
    bottom: PIP_FRAME_INSET.top + geometry.height,
  };

  function center(rect: { left: number; top: number; width: number; height: number }) {
    return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
  }

  it('centers corner zones on the window corners and edge strips on the edges', () => {
    expect(center(pipResizeZoneRect('top-left', geometry))).toEqual({
      x: panel.left,
      y: panel.top,
    });
    expect(center(pipResizeZoneRect('bottom-right', geometry))).toEqual({
      x: panel.right,
      y: panel.bottom,
    });
    for (const edge of PIP_EDGES) {
      const c = center(pipResizeZoneRect(edge, geometry));
      if (edge === 'top' || edge === 'bottom') {
        expect(c.y).toBe(edge === 'top' ? panel.top : panel.bottom);
      } else {
        expect(c.x).toBe(edge === 'left' ? panel.left : panel.right);
      }
    }
  });

  it('keeps every zone inside the frame without overlapping the corners', () => {
    const frame = {
      width: geometry.width + PIP_FRAME_INSET.left + PIP_FRAME_INSET.right,
      height: geometry.height + PIP_FRAME_INSET.top + PIP_FRAME_INSET.bottom,
    };
    for (const handle of [...PIP_EDGES, ...PIP_CORNERS]) {
      const rect = pipResizeZoneRect(handle, geometry);
      expect(rect.left).toBeGreaterThanOrEqual(0);
      expect(rect.top).toBeGreaterThanOrEqual(0);
      expect(rect.left + rect.width).toBeLessThanOrEqual(frame.width);
      expect(rect.top + rect.height).toBeLessThanOrEqual(frame.height);
    }
    const topLeft = pipResizeZoneRect('top-left', geometry);
    expect(pipResizeZoneRect('top', geometry).left).toBe(topLeft.left + topLeft.width);
  });
});

describe('usePipWindow', () => {
  function setViewport(width: number, height: number) {
    Object.defineProperty(window, 'innerWidth', {
      configurable: true,
      writable: true,
      value: width,
    });
    Object.defineProperty(window, 'innerHeight', {
      configurable: true,
      writable: true,
      value: height,
    });
  }

  function pointerEvent(clientX: number, clientY: number): React.PointerEvent {
    return {
      pointerId: 1,
      clientX,
      clientY,
      preventDefault: vi.fn(),
      currentTarget: { setPointerCapture: vi.fn(), releasePointerCapture: vi.fn() },
    } as unknown as React.PointerEvent;
  }

  function keyEvent(key: string, shiftKey = false): React.KeyboardEvent {
    return { key, shiftKey, preventDefault: vi.fn() } as unknown as React.KeyboardEvent;
  }

  function renderPip(sessionId: string | null = 'session-a') {
    return renderHook((props: { active: boolean }) => usePipWindow({ ...props, sessionId }), {
      initialProps: { active: true },
    });
  }

  /** Drags the window up and left so there is room to grow from the corner. */
  function moveAwayFromCorner(hook: ReturnType<typeof renderPip>) {
    act(() => hook.result.current.moveHandlers.onPointerDown(pointerEvent(800, 600)));
    act(() => hook.result.current.moveHandlers.onPointerMove(pointerEvent(500, 400)));
    act(() => hook.result.current.moveHandlers.onPointerUp(pointerEvent(500, 400)));
  }

  beforeEach(() => {
    clearPipGeometryStore();
    setViewport(1200, 900);
  });

  it('applies pointer drags as deltas from the pointer-down position', () => {
    const hook = renderPip();
    moveAwayFromCorner(hook);
    const { result } = hook;
    const before = pipTopLeft(result.current.geometry, viewport);

    act(() => result.current.resizeHandlers['bottom-right'].onPointerDown(pointerEvent(1000, 700)));
    act(() => result.current.resizeHandlers['bottom-right'].onPointerMove(pointerEvent(1150, 780)));
    act(() => result.current.resizeHandlers['bottom-right'].onPointerUp(pointerEvent(1150, 780)));

    expect(result.current.geometry.width).toBe(PIP_MIN_WIDTH + 150);
    expect(result.current.geometry.height).toBe(PIP_MIN_HEIGHT + 80);
    expect(pipTopLeft(result.current.geometry, viewport)).toEqual(before);
  });

  it('steps with arrow keys, 8px or 32px with shift, and never drops below the minimum', () => {
    const hook = renderPip();
    moveAwayFromCorner(hook);
    const { result } = hook;
    const handle = () => result.current.resizeHandlers['bottom-right'];

    act(() => handle().onKeyDown(keyEvent('ArrowRight', true)));
    act(() => handle().onKeyDown(keyEvent('ArrowDown')));
    expect(result.current.geometry.width).toBe(PIP_MIN_WIDTH + 32);
    expect(result.current.geometry.height).toBe(PIP_MIN_HEIGHT + 8);

    act(() => handle().onKeyDown(keyEvent('ArrowLeft', true)));
    act(() => handle().onKeyDown(keyEvent('ArrowLeft', true)));
    act(() => handle().onKeyDown(keyEvent('ArrowUp')));
    act(() => handle().onKeyDown(keyEvent('ArrowUp')));
    expect(result.current.geometry.width).toBe(PIP_MIN_WIDTH);
    expect(result.current.geometry.height).toBe(PIP_MIN_HEIGHT);
  });

  it('re-clamps when the main window shrinks', () => {
    const hook = renderPip();
    moveAwayFromCorner(hook);
    const { result } = hook;

    for (let i = 0; i < 3; i++) {
      act(() =>
        result.current.resizeHandlers['bottom-right'].onKeyDown(keyEvent('ArrowRight', true))
      );
    }
    expect(result.current.geometry.width).toBe(PIP_MIN_WIDTH + 96);

    setViewport(450, 900);
    act(() => {
      window.dispatchEvent(new Event('resize'));
    });

    expect(result.current.geometry.width).toBe(450 - PIP_FRAME_INSET.left - PIP_FRAME_INSET.right);
    const { left } = pipTopLeft(result.current.geometry, { width: 450, height: 900 });
    expect(left).toBeGreaterThanOrEqual(PIP_FRAME_INSET.left);
  });

  it('reopens at the last geometry within a session and at the defaults for a new session', () => {
    const first = renderPip('session-a');
    act(() =>
      first.result.current.resizeHandlers['bottom-right'].onKeyDown(keyEvent('ArrowRight', true))
    );
    act(() => first.result.current.moveHandlers.onKeyDown(keyEvent('ArrowUp', true)));
    const remembered = first.result.current.geometry;
    expect(remembered).not.toEqual(DEFAULT_PIP_GEOMETRY);

    first.rerender({ active: false });
    first.rerender({ active: true });
    expect(first.result.current.geometry).toEqual(remembered);
    first.unmount();

    expect(renderPip('session-a').result.current.geometry).toEqual(remembered);
    expect(renderPip('session-b').result.current.geometry).toEqual(DEFAULT_PIP_GEOMETRY);
  });
});
