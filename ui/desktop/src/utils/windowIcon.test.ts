import { describe, expect, it } from 'vitest';
import { getWindowIconFilename } from './windowIcon';

describe('getWindowIconFilename', () => {
  it.each([
    ['darwin', 'icon.icns'],
    ['win32', 'icon.ico'],
    ['linux', 'icon.png'],
  ] as const)('uses the native icon for %s', (platform, expected) => {
    expect(getWindowIconFilename(platform)).toBe(expected);
  });

  it('uses the cross-platform PNG fallback', () => {
    expect(getWindowIconFilename('freebsd')).toBe('icon.png');
  });
});
