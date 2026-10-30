import { describe, expect, it, vi } from 'vitest';

vi.mock('./logger', () => ({ default: { info: vi.fn(), error: vi.fn() } }));

import { appendToWindowsPath } from './winShims';

const shims = 'C:\\Users\\me\\AppData\\Local\\Goose\\bin';

describe('appendToWindowsPath', () => {
  it.each([
    [
      'puts the shims after the user PATH',
      'C:\\nodejs;C:\\Windows',
      `C:\\nodejs;C:\\Windows;${shims}`,
    ],
    ['handles an empty PATH', '', shims],
    [
      'leaves the shims where they are when already present',
      `${shims};C:\\nodejs`,
      `${shims};C:\\nodejs`,
    ],
    [
      'matches case-insensitively',
      `C:\\nodejs;${shims.toUpperCase()}`,
      `C:\\nodejs;${shims.toUpperCase()}`,
    ],
    ['does not mistake a longer entry for the shims', `${shims}2`, `${shims}2;${shims}`],
  ])('%s', (_name, currentPath, expected) => {
    expect(appendToWindowsPath(currentPath, shims)).toBe(expected);
  });
});
