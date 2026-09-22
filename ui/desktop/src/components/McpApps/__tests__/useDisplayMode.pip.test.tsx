import type React from 'react';
import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  DEFAULT_PIP_GEOMETRY,
  PIP_FRAME_INSET,
  PIP_MIN_HEIGHT,
  PIP_MIN_WIDTH,
  clearPipGeometryStore,
  pipTopLeft,
} from '../pipGeometry';
import { useDisplayMode } from '../useDisplayMode';

function setViewport(width: number, height: number) {
  Object.defineProperty(window, 'innerWidth', { configurable: true, writable: true, value: width });
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
  const containerRef = { current: null };
  const hook = renderHook(() => useDisplayMode({ displayMode: 'inline', containerRef, sessionId }));
  act(() => hook.result.current.changeDisplayMode('pip'));
  return hook;
}

/** Drags the window up and left so there is room to grow from the corner. */
function moveAwayFromCorner(hook: ReturnType<typeof renderPip>) {
  act(() => hook.result.current.pipHandlers.onPointerDown(pointerEvent(800, 600)));
  act(() => hook.result.current.pipHandlers.onPointerMove(pointerEvent(500, 400)));
  act(() => hook.result.current.pipHandlers.onPointerUp(pointerEvent(500, 400)));
}

describe('useDisplayMode PiP geometry', () => {
  beforeEach(() => {
    clearPipGeometryStore();
    setViewport(1200, 900);
    window.matchMedia = vi
      .fn()
      .mockReturnValue({ matches: true }) as unknown as typeof window.matchMedia;
  });

  it('opens at the default size and position', () => {
    const { result } = renderPip();
    expect(result.current.isPip).toBe(true);
    expect(result.current.pipGeometry).toEqual(DEFAULT_PIP_GEOMETRY);
  });

  it('resizes from the bottom-right corner with the pointer, keeping the top-left corner fixed', () => {
    const hook = renderPip();
    moveAwayFromCorner(hook);
    const { result } = hook;
    const viewport = { width: 1200, height: 900 };
    const before = pipTopLeft(result.current.pipGeometry, viewport);

    act(() =>
      result.current.pipResizeHandlers['bottom-right'].onPointerDown(pointerEvent(1000, 700))
    );
    act(() =>
      result.current.pipResizeHandlers['bottom-right'].onPointerMove(pointerEvent(1150, 780))
    );
    act(() =>
      result.current.pipResizeHandlers['bottom-right'].onPointerUp(pointerEvent(1150, 780))
    );

    expect(result.current.pipGeometry.width).toBe(PIP_MIN_WIDTH + 150);
    expect(result.current.pipGeometry.height).toBe(PIP_MIN_HEIGHT + 80);
    expect(pipTopLeft(result.current.pipGeometry, viewport)).toEqual(before);
  });

  it('resizes with arrow keys and never drops below the minimum size', () => {
    const hook = renderPip();
    moveAwayFromCorner(hook);
    const { result } = hook;

    act(() =>
      result.current.pipResizeHandlers['bottom-right'].onKeyDown(keyEvent('ArrowRight', true))
    );
    act(() => result.current.pipResizeHandlers['bottom-right'].onKeyDown(keyEvent('ArrowDown')));
    expect(result.current.pipGeometry.width).toBe(PIP_MIN_WIDTH + 32);
    expect(result.current.pipGeometry.height).toBe(PIP_MIN_HEIGHT + 8);

    act(() =>
      result.current.pipResizeHandlers['bottom-right'].onKeyDown(keyEvent('ArrowLeft', true))
    );
    act(() =>
      result.current.pipResizeHandlers['bottom-right'].onKeyDown(keyEvent('ArrowLeft', true))
    );
    act(() => result.current.pipResizeHandlers['bottom-right'].onKeyDown(keyEvent('ArrowUp')));
    act(() => result.current.pipResizeHandlers['bottom-right'].onKeyDown(keyEvent('ArrowUp')));
    expect(result.current.pipGeometry.width).toBe(PIP_MIN_WIDTH);
    expect(result.current.pipGeometry.height).toBe(PIP_MIN_HEIGHT);
  });

  it('resizes from the top-left corner, keeping the bottom-right corner fixed', () => {
    const hook = renderPip();
    moveAwayFromCorner(hook);
    const { result } = hook;
    const before = result.current.pipGeometry;

    act(() => result.current.pipResizeHandlers['top-left'].onPointerDown(pointerEvent(500, 400)));
    act(() => result.current.pipResizeHandlers['top-left'].onPointerMove(pointerEvent(400, 350)));
    act(() => result.current.pipResizeHandlers['top-left'].onPointerUp(pointerEvent(400, 350)));

    expect(result.current.pipGeometry).toEqual({
      x: before.x,
      y: before.y,
      width: PIP_MIN_WIDTH + 100,
      height: PIP_MIN_HEIGHT + 50,
    });
  });

  it('moves with the move handle without changing size', () => {
    const { result } = renderPip();

    act(() => result.current.pipHandlers.onPointerDown(pointerEvent(800, 600)));
    act(() => result.current.pipHandlers.onPointerMove(pointerEvent(700, 500)));

    expect(result.current.pipGeometry).toEqual({
      ...DEFAULT_PIP_GEOMETRY,
      x: -100,
      y: -100,
    });
  });

  it('cannot be dragged or resized beyond the main window', () => {
    const { result } = renderPip();
    const viewport = { width: 1200, height: 900 };

    act(() => result.current.pipHandlers.onPointerDown(pointerEvent(0, 0)));
    act(() => result.current.pipHandlers.onPointerMove(pointerEvent(-5000, -5000)));
    expect(pipTopLeft(result.current.pipGeometry, viewport)).toEqual({
      left: PIP_FRAME_INSET.left,
      top: PIP_FRAME_INSET.top,
    });

    act(() => result.current.pipResizeHandlers['bottom-right'].onPointerDown(pointerEvent(0, 0)));
    act(() =>
      result.current.pipResizeHandlers['bottom-right'].onPointerMove(pointerEvent(5000, 5000))
    );
    expect(result.current.pipGeometry.width).toBe(
      viewport.width - PIP_FRAME_INSET.left - PIP_FRAME_INSET.right
    );
    expect(result.current.pipGeometry.height).toBe(
      viewport.height - PIP_FRAME_INSET.top - PIP_FRAME_INSET.bottom
    );
  });

  it('shrinks and re-clamps when the main window is resized', () => {
    const hook = renderPip();
    moveAwayFromCorner(hook);
    const { result } = hook;

    act(() =>
      result.current.pipResizeHandlers['bottom-right'].onKeyDown(keyEvent('ArrowRight', true))
    );
    act(() =>
      result.current.pipResizeHandlers['bottom-right'].onKeyDown(keyEvent('ArrowRight', true))
    );
    act(() =>
      result.current.pipResizeHandlers['bottom-right'].onKeyDown(keyEvent('ArrowRight', true))
    );
    expect(result.current.pipGeometry.width).toBe(PIP_MIN_WIDTH + 96);

    setViewport(450, 900);
    act(() => {
      window.dispatchEvent(new Event('resize'));
    });

    expect(result.current.pipGeometry.width).toBe(
      450 - PIP_FRAME_INSET.left - PIP_FRAME_INSET.right
    );
    const { left } = pipTopLeft(result.current.pipGeometry, { width: 450, height: 900 });
    expect(left).toBeGreaterThanOrEqual(PIP_FRAME_INSET.left);
  });

  it('reopens at the last size and position within the same session', () => {
    const first = renderPip('session-a');
    act(() =>
      first.result.current.pipResizeHandlers['bottom-right'].onKeyDown(keyEvent('ArrowRight', true))
    );
    act(() => first.result.current.pipHandlers.onKeyDown(keyEvent('ArrowUp', true)));
    const remembered = first.result.current.pipGeometry;
    expect(remembered).not.toEqual(DEFAULT_PIP_GEOMETRY);

    act(() => first.result.current.changeDisplayMode('inline'));
    act(() => first.result.current.changeDisplayMode('pip'));
    expect(first.result.current.pipGeometry).toEqual(remembered);
    first.unmount();

    const second = renderPip('session-a');
    expect(second.result.current.pipGeometry).toEqual(remembered);
  });

  it('uses the defaults for a different session', () => {
    const first = renderPip('session-a');
    act(() =>
      first.result.current.pipResizeHandlers['bottom-right'].onKeyDown(keyEvent('ArrowRight', true))
    );
    first.unmount();

    const second = renderPip('session-b');
    expect(second.result.current.pipGeometry).toEqual(DEFAULT_PIP_GEOMETRY);
  });
});
