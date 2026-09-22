import { describe, expect, it } from 'vitest';
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
  movePipGeometry,
  pipTopLeft,
  pipResizeZoneRect,
  resizePipGeometry,
} from '../pipGeometry';

const viewport = { width: 1200, height: 900 };

describe('clampPipGeometry', () => {
  it('leaves the default geometry untouched in a large viewport', () => {
    expect(clampPipGeometry(DEFAULT_PIP_GEOMETRY, viewport)).toEqual(DEFAULT_PIP_GEOMETRY);
  });

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
    const { left, top } = pipTopLeft(farRightDown, viewport);
    expect(left + farRightDown.width).toBe(viewport.width - PIP_FRAME_INSET.right);
    expect(top + farRightDown.height).toBe(viewport.height - PIP_FRAME_INSET.bottom);
  });

  it('falls back to the anchored offset when the viewport is smaller than the window', () => {
    const tiny = { width: 300, height: 200 };
    const result = clampPipGeometry({ x: 50, y: 50, width: 800, height: 600 }, tiny);
    expect(result).toEqual({ x: 0, y: 0, width: PIP_MIN_WIDTH, height: PIP_MIN_HEIGHT });
  });
});

describe('movePipGeometry', () => {
  it('shifts position without changing size', () => {
    const result = movePipGeometry(DEFAULT_PIP_GEOMETRY, -100, -50, viewport);
    expect(result).toEqual({ ...DEFAULT_PIP_GEOMETRY, x: -100, y: -50 });
  });

  it('clamps to the viewport', () => {
    const result = movePipGeometry(DEFAULT_PIP_GEOMETRY, 1000, 1000, viewport);
    expect(result.x).toBe(PIP_MARGIN_RIGHT - PIP_FRAME_INSET.right);
    expect(result.y).toBe(PIP_MARGIN_BOTTOM - PIP_FRAME_INSET.bottom);
  });
});

describe('resizePipGeometry', () => {
  const movedAwayFromCorner = { ...DEFAULT_PIP_GEOMETRY, x: -300, y: -200 };

  function corners(geometry: typeof DEFAULT_PIP_GEOMETRY) {
    const { left, top } = pipTopLeft(geometry, viewport);
    return { left, top, right: left + geometry.width, bottom: top + geometry.height };
  }

  it('grows from the bottom-right corner while keeping the top-left corner fixed', () => {
    const before = corners(movedAwayFromCorner);
    const result = resizePipGeometry(movedAwayFromCorner, 120, 80, viewport, 'bottom-right');
    const after = corners(result);

    expect(result.width).toBe(PIP_MIN_WIDTH + 120);
    expect(result.height).toBe(PIP_MIN_HEIGHT + 80);
    expect(after.left).toBe(before.left);
    expect(after.top).toBe(before.top);
  });

  it('grows from the top-left corner while keeping the bottom-right corner fixed', () => {
    const before = corners(movedAwayFromCorner);
    const result = resizePipGeometry(movedAwayFromCorner, -120, -80, viewport, 'top-left');
    const after = corners(result);

    expect(result.width).toBe(PIP_MIN_WIDTH + 120);
    expect(result.height).toBe(PIP_MIN_HEIGHT + 80);
    expect(after.right).toBe(before.right);
    expect(after.bottom).toBe(before.bottom);
  });

  it('grows from the top-right corner while keeping the bottom-left corner fixed', () => {
    const before = corners(movedAwayFromCorner);
    const result = resizePipGeometry(movedAwayFromCorner, 120, -80, viewport, 'top-right');
    const after = corners(result);

    expect(result.width).toBe(PIP_MIN_WIDTH + 120);
    expect(result.height).toBe(PIP_MIN_HEIGHT + 80);
    expect(after.left).toBe(before.left);
    expect(after.bottom).toBe(before.bottom);
  });

  it('grows from the bottom-left corner while keeping the top-right corner fixed', () => {
    const before = corners(movedAwayFromCorner);
    const result = resizePipGeometry(movedAwayFromCorner, -120, 80, viewport, 'bottom-left');
    const after = corners(result);

    expect(result.width).toBe(PIP_MIN_WIDTH + 120);
    expect(result.height).toBe(PIP_MIN_HEIGHT + 80);
    expect(after.right).toBe(before.right);
    expect(after.top).toBe(before.top);
  });

  it('only grows as far as the frame inset from the default anchored position', () => {
    const result = resizePipGeometry(DEFAULT_PIP_GEOMETRY, 500, 0, viewport, 'bottom-right');
    expect(result.width).toBe(PIP_MIN_WIDTH + PIP_MARGIN_RIGHT - PIP_FRAME_INSET.right);
    expect(pipTopLeft(result, viewport).left + result.width).toBe(
      viewport.width - PIP_FRAME_INSET.right
    );
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
      const result = resizePipGeometry(movedAwayFromCorner, 0, 0, viewport, corner);
      const shrunk = resizePipGeometry(
        result,
        corner.endsWith('left') ? 500 : -500,
        corner.startsWith('top') ? 500 : -500,
        viewport,
        corner
      );
      expect(shrunk.width).toBe(PIP_MIN_WIDTH);
      expect(shrunk.height).toBe(PIP_MIN_HEIGHT);
    }
  });

  it('stops growing at the viewport edges', () => {
    const before = pipTopLeft(DEFAULT_PIP_GEOMETRY, viewport);
    const result = resizePipGeometry(DEFAULT_PIP_GEOMETRY, 5000, 5000, viewport, 'bottom-right');
    const after = pipTopLeft(result, viewport);

    expect(after).toEqual(before);
    expect(after.left + result.width).toBe(viewport.width - PIP_FRAME_INSET.right);
    expect(after.top + result.height).toBe(viewport.height - PIP_FRAME_INSET.bottom);
  });

  it('stops the top-left corner at the frame inset', () => {
    const before = corners(movedAwayFromCorner);
    const result = resizePipGeometry(movedAwayFromCorner, -5000, -5000, viewport, 'top-left');
    const after = corners(result);

    expect(after.left).toBe(PIP_FRAME_INSET.left);
    expect(after.top).toBe(PIP_FRAME_INSET.top);
    expect(after.right).toBe(before.right);
    expect(after.bottom).toBe(before.bottom);
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

  it('centers each corner zone on its window corner, above the title bar at the top', () => {
    expect(center(pipResizeZoneRect('top-left', geometry))).toEqual({
      x: panel.left,
      y: panel.top,
    });
    expect(center(pipResizeZoneRect('bottom-right', geometry))).toEqual({
      x: panel.right,
      y: panel.bottom,
    });
  });

  it('centers each edge strip on its panel edge without overlapping the corners', () => {
    for (const edge of PIP_EDGES) {
      const rect = pipResizeZoneRect(edge, geometry);
      const c = center(rect);
      if (edge === 'top' || edge === 'bottom') {
        expect(c.y).toBe(edge === 'top' ? panel.top : panel.bottom);
      } else {
        expect(c.x).toBe(edge === 'left' ? panel.left : panel.right);
      }
    }
    const topLeft = pipResizeZoneRect('top-left', geometry);
    const top = pipResizeZoneRect('top', geometry);
    expect(top.left).toBe(topLeft.left + topLeft.width);
  });

  it('keeps every zone inside the frame', () => {
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
  });
});
