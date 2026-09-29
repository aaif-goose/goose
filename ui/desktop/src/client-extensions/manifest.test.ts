import { describe, expect, it } from 'vitest';
import { parseClientExtensionManifest, satisfiesGrcEngine } from './manifest';

const base = { id: 'demo-ext', version: '0.1.0', main: 'index.html' };

describe('parseClientExtensionManifest', () => {
  it('parses the required fields', () => {
    expect(parseClientExtensionManifest(base)).toEqual(base);
  });

  it('rejects manifests without valid id, version or main', () => {
    expect(parseClientExtensionManifest(null)).toBeNull();
    expect(parseClientExtensionManifest([])).toBeNull();
    expect(parseClientExtensionManifest({ ...base, id: '' })).toBeNull();
    expect(parseClientExtensionManifest({ ...base, id: '../evil' })).toBeNull();
    expect(parseClientExtensionManifest({ ...base, version: ' ' })).toBeNull();
    expect(parseClientExtensionManifest({ ...base, main: 3 })).toBeNull();
  });

  it('keeps known permissions, dedupes them and drops unknown ones', () => {
    const manifest = parseClientExtensionManifest({
      ...base,
      permissions: ['sessions:read', 'sessions:read', 'providers:write', 'root:everything', 7],
    });

    expect(manifest?.permissions).toEqual(['sessions:read', 'providers:write']);
  });

  it('omits permissions when none are valid', () => {
    expect(
      parseClientExtensionManifest({ ...base, permissions: ['nope'] })?.permissions
    ).toBeUndefined();
  });

  it('parses contributions and drops malformed entries', () => {
    const manifest = parseClientExtensionManifest({
      ...base,
      contributes: {
        chatActions: [{ id: 'a', label: 'A', when: 'session.active' }, { id: 'bad' }],
        rootLinks: [{ id: 'home', label: 'Home' }],
        sidecars: [{ id: 's', label: 'S', defaultOpen: true }],
        contentSuffixes: [{ id: 'badge' }],
        customRenders: [
          { id: 'json', match: { contentType: 'code', language: ' JSON ' }, priority: 5 },
          { id: 'no-match', match: {} },
        ],
      },
    });

    expect(manifest?.contributes).toEqual({
      chatActions: [{ id: 'a', label: 'A', when: 'session.active' }],
      rootLinks: [{ id: 'home', label: 'Home', when: undefined }],
      contentSuffixes: [{ id: 'badge', when: undefined }],
      customRenders: [
        { id: 'json', match: { contentType: 'code', language: 'json' }, priority: 5 },
      ],
      sidecars: [{ id: 's', label: 'S', defaultOpen: true }],
    });
  });
});

describe('theme contributions', () => {
  const theme = {
    id: 'midnight',
    label: ' Midnight ',
    variant: 'dark',
    tokens: { '--color-background-primary': '#010203', '--bad': 7 },
  };

  it('parses themes and keeps only string token values', () => {
    const manifest = parseClientExtensionManifest({ ...base, contributes: { themes: [theme] } });

    expect(manifest?.contributes?.themes).toEqual([
      {
        id: 'midnight',
        label: 'Midnight',
        variant: 'dark',
        tokens: { '--color-background-primary': '#010203' },
      },
    ]);
  });

  it('drops themes with an unsafe id, an unknown variant, a blank label or no tokens', () => {
    const manifest = parseClientExtensionManifest({
      ...base,
      contributes: {
        themes: [
          { ...theme, id: '../evil' },
          { ...theme, variant: 'sepia' },
          { ...theme, label: ' ' },
          { ...theme, tokens: undefined },
        ],
      },
    });

    expect(manifest?.contributes).toBeUndefined();
  });
});

describe('satisfiesGrcEngine', () => {
  it('accepts manifests without an engine constraint', () => {
    expect(satisfiesGrcEngine(base, '1.52.0')).toBe(true);
  });

  it('supports a bare minimum version and range expressions', () => {
    expect(satisfiesGrcEngine({ ...base, engines: { grc: '1.40.0' } }, '1.52.0')).toBe(true);
    expect(satisfiesGrcEngine({ ...base, engines: { grc: '1.60.0' } }, '1.52.0')).toBe(false);
    expect(satisfiesGrcEngine({ ...base, engines: { grc: '>=1.40.0' } }, '1.52.0')).toBe(true);
    expect(satisfiesGrcEngine({ ...base, engines: { grc: '>=2.0.0' } }, '1.52.0')).toBe(false);
  });
});
