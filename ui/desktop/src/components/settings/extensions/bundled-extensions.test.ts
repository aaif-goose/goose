import { describe, it, expect, vi } from 'vitest';
import { pruneDeprecatedBundledExtensions, syncBundledExtensions } from './bundled-extensions';
import type { FixedExtensionEntry } from '../../ConfigContext';

vi.mock('./bundled-extensions.json', () => ({
  default: [
    {
      id: 'developer',
      name: 'developer',
      display_name: 'Developer',
      description: 'General development tools.',
      enabled: true,
      type: 'builtin',
      timeout: 300,
    },
    {
      id: 'googledrive',
      name: 'googledrive',
      display_name: 'Google Drive',
      description: 'Google Drive integration.',
      enabled: true,
      type: 'stdio',
      cmd: 'googledrive-mcp',
      args: [],
      env_keys: [],
      timeout: 300,
    },
  ],
}));

vi.mock('./deprecated-bundled-extensions.json', () => ({
  default: [{ id: 'googledrive' }, { id: 'old-bundled-extension' }],
}));

describe('syncBundledExtensions', () => {
  it('skips already bundled non-deprecated extensions', async () => {
    const addExtensionFn = vi.fn().mockResolvedValue(undefined);
    const existingExtensions = [
      {
        name: 'developer',
        type: 'builtin',
        description: 'Developer tools',
        enabled: true,
        bundled: true,
        timeout: 300,
      },
    ] as FixedExtensionEntry[];

    await syncBundledExtensions(existingExtensions, addExtensionFn);

    expect(addExtensionFn).not.toHaveBeenCalledWith(
      'developer',
      expect.anything(),
      expect.anything()
    );
  });
});

describe('pruneDeprecatedBundledExtensions', () => {
  it('removes deprecated bundled extensions', async () => {
    const removeExtensionFn = vi.fn().mockResolvedValue(undefined);
    const existingExtensions = [
      {
        name: 'old-bundled-extension',
        type: 'builtin',
        description: 'Old bundled extension',
        enabled: true,
        bundled: true,
      },
    ] as FixedExtensionEntry[];

    await pruneDeprecatedBundledExtensions(existingExtensions, removeExtensionFn);

    expect(removeExtensionFn).toHaveBeenCalledWith('old-bundled-extension');
  });

  it('does not remove non-bundled deprecated extensions', async () => {
    const removeExtensionFn = vi.fn().mockResolvedValue(undefined);
    const existingExtensions = [
      {
        name: 'old-bundled-extension',
        type: 'builtin',
        description: 'Old bundled extension',
        enabled: true,
        bundled: false,
      },
    ] as FixedExtensionEntry[];

    await pruneDeprecatedBundledExtensions(existingExtensions, removeExtensionFn);

    expect(removeExtensionFn).not.toHaveBeenCalled();
  });

  it('removes deprecated bundled googledrive extensions', async () => {
    const removeExtensionFn = vi.fn().mockResolvedValue(undefined);
    const existingExtensions = [
      {
        name: 'Google Drive',
        type: 'stdio',
        description: 'Google Drive extension',
        cmd: 'some-cmd',
        args: [],
        env_keys: [],
        enabled: true,
        bundled: true,
      },
    ] as FixedExtensionEntry[];

    await pruneDeprecatedBundledExtensions(existingExtensions, removeExtensionFn);

    expect(removeExtensionFn).toHaveBeenCalledWith('googledrive');
  });
});
