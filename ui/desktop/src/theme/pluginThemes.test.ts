/**
 * @vitest-environment jsdom
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  applyThemeTokens,
  buildMcpHostStyles,
  darkTokens,
  getPluginThemeOptions,
  getThemeDefinition,
  hasTheme,
  lightTokens,
  pluginThemeId,
  registerPluginThemes,
  subscribePluginThemes,
} from './theme-tokens';

const midnight = {
  extensionId: 'demo',
  id: 'midnight',
  label: 'Midnight',
  variant: 'dark' as const,
  tokens: {
    '--color-background-primary': '#010203',
    '--color-text-primary': 'url(https://evil.example/x)',
    '--color-border-primary': 'red; background: blue',
    '--not-a-token': '#ffffff',
  },
};

afterEach(() => {
  registerPluginThemes([]);
});

describe('plugin themes', () => {
  it('namespaces the theme id with the extension id', () => {
    expect(pluginThemeId('demo', 'midnight')).toBe('demo:midnight');
  });

  it('overlays valid tokens on the built-in palette of the same variant', () => {
    registerPluginThemes([midnight]);
    const { tokens, variant } = getThemeDefinition('demo:midnight');

    expect(variant).toBe('dark');
    expect(tokens['--color-background-primary']).toBe('#010203');
    expect(tokens['--color-text-secondary']).toBe(darkTokens['--color-text-secondary']);
  });

  it('drops unsafe values and unknown token names', () => {
    registerPluginThemes([midnight]);
    const { tokens } = getThemeDefinition('demo:midnight');

    expect(tokens['--color-text-primary']).toBe(darkTokens['--color-text-primary']);
    expect(tokens['--color-border-primary']).toBe(darkTokens['--color-border-primary']);
    expect(Object.keys(tokens)).not.toContain('--not-a-token');
  });

  it('uses the light palette as the base for light themes', () => {
    registerPluginThemes([{ ...midnight, id: 'sunrise', variant: 'light' }]);

    expect(getThemeDefinition('demo:sunrise').tokens['--color-text-secondary']).toBe(
      lightTokens['--color-text-secondary']
    );
  });

  it('knows built-in and registered themes and falls back to light for unknown ones', () => {
    registerPluginThemes([midnight]);

    expect(hasTheme('aura')).toBe(true);
    expect(hasTheme('demo:midnight')).toBe(true);
    expect(hasTheme('constructor')).toBe(false);
    expect(hasTheme('missing:theme')).toBe(false);
    expect(getThemeDefinition('missing:theme').variant).toBe('light');
    expect(getThemeDefinition('constructor').variant).toBe('light');
  });

  it('replaces the previous registration and notifies subscribers', () => {
    const listener = vi.fn();
    const unsubscribe = subscribePluginThemes(listener);

    registerPluginThemes([midnight]);
    const first = getPluginThemeOptions();
    expect(getPluginThemeOptions()).toBe(first);
    expect(first).toEqual([{ id: 'demo:midnight', label: 'Midnight', variant: 'dark' }]);

    registerPluginThemes([]);
    expect(getPluginThemeOptions()).toEqual([]);
    expect(hasTheme('demo:midnight')).toBe(false);
    expect(listener).toHaveBeenCalledTimes(2);

    unsubscribe();
    registerPluginThemes([midnight]);
    expect(listener).toHaveBeenCalledTimes(2);
  });

  it('applies the palette to the document root and to MCP app styles', () => {
    registerPluginThemes([midnight]);
    applyThemeTokens('demo:midnight');

    expect(document.documentElement.style.getPropertyValue('--color-background-primary')).toBe(
      '#010203'
    );

    const styles = buildMcpHostStyles('demo:midnight');
    expect(styles.variables?.['--color-background-primary']).toBe('#010203');
    expect(styles.variables?.['--color-text-secondary']).not.toContain('light-dark(');
  });
});
