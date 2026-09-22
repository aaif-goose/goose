import { describe, it, expect } from 'vitest';
import { getContainerDimensions } from '../containerDimensions';

describe('getContainerDimensions', () => {
  it('reports only width for inline so the guest controls its height', () => {
    expect(getContainerDimensions('inline', 640, 200)).toEqual({ width: 640 });
  });

  it('reports width and height for fullscreen', () => {
    expect(getContainerDimensions('fullscreen', 1280, 752)).toEqual({ width: 1280, height: 752 });
  });

  it('reports width and height for standalone windows', () => {
    expect(getContainerDimensions('standalone', 900, 600)).toEqual({ width: 900, height: 600 });
  });

  it('reports width and maxHeight for pip because the pip window scrolls its content', () => {
    const dimensions = getContainerDimensions('pip', 398, 298);
    expect(dimensions).toEqual({ width: 398, maxHeight: 298 });
    expect(dimensions).not.toHaveProperty('height');
  });

  it('returns undefined until a fixed axis has been measured', () => {
    expect(getContainerDimensions('inline', 0, 200)).toBeUndefined();
    expect(getContainerDimensions('fullscreen', 1280, 0)).toBeUndefined();
  });

  it('ignores a missing measurement on an unbounded axis', () => {
    expect(getContainerDimensions('inline', 640, 0)).toEqual({ width: 640 });
  });
});
