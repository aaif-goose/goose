import { describe, expect, it } from 'vitest';
import type { FixedExtensionEntry } from '../../ConfigContext';
import { sortExtensionsBySourcePriority } from './extensionCategories';

describe('extensionCategories', () => {
  it('sorts builtin extensions before bundled extensions before custom extensions', () => {
    const extensions = [
      {
        name: 'z-custom',
        type: 'stdio',
        description: 'Custom',
        cmd: 'custom',
        enabled: false,
      },
      {
        name: 'b-bundled',
        type: 'stdio',
        description: 'Bundled',
        cmd: 'bundled',
        enabled: false,
        bundled: true,
      },
      {
        name: 'z-builtin',
        type: 'builtin',
        description: 'Built in',
        enabled: true,
      },
      {
        name: 'a-builtin',
        type: 'builtin',
        description: 'Built in',
        enabled: true,
      },
      {
        name: 'a-custom',
        type: 'stdio',
        description: 'Custom',
        cmd: 'custom',
        enabled: false,
      },
    ] as FixedExtensionEntry[];

    expect(sortExtensionsBySourcePriority(extensions).map((extension) => extension.name)).toEqual([
      'a-builtin',
      'z-builtin',
      'b-bundled',
      'a-custom',
      'z-custom',
    ]);
  });
});
