import { describe, it, expect } from 'vitest';
import { getContainerDimensions } from '../containerDimensions';

describe('getContainerDimensions', () => {
  it('reports only width for inline so the guest controls its height', () => {
    expect(getContainerDimensions('inline', 640, 200)).toEqual({ width: 640 });
    expect(getContainerDimensions('inline', 640, 0)).toEqual({ width: 640 });
  });

  it('reports width and height for modes that fill the viewport', () => {
    expect(getContainerDimensions('fullscreen', 1280, 752)).toEqual({ width: 1280, height: 752 });
    expect(getContainerDimensions('standalone', 900, 600)).toEqual({ width: 900, height: 600 });
  });

  it('reports width and maxHeight for pip because the pip window scrolls its content', () => {
    expect(getContainerDimensions('pip', 398, 298)).toEqual({ width: 398, maxHeight: 298 });
  });

  it('returns undefined until a fixed axis has been measured', () => {
    expect(getContainerDimensions('inline', 0, 200)).toBeUndefined();
    expect(getContainerDimensions('fullscreen', 1280, 0)).toBeUndefined();
  });
});
