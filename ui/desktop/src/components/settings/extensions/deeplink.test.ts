import { describe, expect, it, vi } from 'vitest';

vi.mock('../../../toasts', () => ({
  toastService: {
    handleError: vi.fn(),
    success: vi.fn(),
  },
}));

import {
  FILESYSTEM_WORKING_DIR_PLACEHOLDER,
  normalizeFilesystemInstallArgs,
} from './deeplink';

describe('normalizeFilesystemInstallArgs', () => {
  it('replaces legacy filesystem placeholders with the working dir token', () => {
    expect(
      normalizeFilesystemInstallArgs([
        '-y',
        '@modelcontextprotocol/server-filesystem',
        '/path/to/dir1',
        '/path/to/dir2',
      ])
    ).toEqual([
      '-y',
      '@modelcontextprotocol/server-filesystem',
      FILESYSTEM_WORKING_DIR_PLACEHOLDER,
    ]);
  });

  it('preserves explicit filesystem paths', () => {
    expect(
      normalizeFilesystemInstallArgs([
        '-y',
        '@modelcontextprotocol/server-filesystem',
        '/Users/example/projects/goose',
      ])
    ).toEqual([
      '-y',
      '@modelcontextprotocol/server-filesystem',
      '/Users/example/projects/goose',
    ]);
  });

  it('leaves non-filesystem extensions unchanged', () => {
    expect(normalizeFilesystemInstallArgs(['-y', '@modelcontextprotocol/server-postgres'])).toEqual(
      ['-y', '@modelcontextprotocol/server-postgres']
    );
  });
});
