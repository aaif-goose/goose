/**
 * @vitest-environment jsdom
 */
import React from 'react';
import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { registerPluginThemes } from '../theme/theme-tokens';
import { ThemeProvider, useTheme } from './ThemeContext';

const theme = {
  extensionId: 'demo',
  id: 'midnight',
  label: 'Midnight',
  variant: 'dark' as const,
  tokens: { '--color-background-primary': '#010203' },
};

const getSetting = vi.fn();
const setSetting = vi.fn().mockResolvedValue(undefined);

function wrapper({ children }: { children: React.ReactNode }) {
  return <ThemeProvider>{children}</ThemeProvider>;
}

beforeEach(() => {
  getSetting.mockImplementation(async (key: string) =>
    key === 'useSystemTheme' ? false : 'demo:midnight'
  );
  (window as unknown as { electron: unknown }).electron = {
    getSetting,
    setSetting,
    on: vi.fn(),
    off: vi.fn(),
    broadcastThemeChange: vi.fn(),
  };
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    value: vi.fn().mockImplementation(() => ({
      matches: false,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    })),
  });
});

afterEach(() => {
  registerPluginThemes([]);
  vi.clearAllMocks();
});

describe('ThemeProvider with plugin themes', () => {
  it('keeps a saved plugin theme and applies it once the plugin registers', async () => {
    const { result } = renderHook(() => useTheme(), { wrapper });

    await waitFor(() => expect(result.current.userThemePreference).toBe('demo:midnight'));
    expect(result.current.resolvedThemeId).toBe('light');

    act(() => registerPluginThemes([theme]));

    await waitFor(() => expect(result.current.resolvedThemeId).toBe('demo:midnight'));
    expect(result.current.resolvedTheme).toBe('dark');
    expect(document.documentElement.dataset.theme).toBe('demo:midnight');
    expect(document.documentElement.style.getPropertyValue('--color-background-primary')).toBe(
      '#010203'
    );
    expect(result.current.pluginThemes).toEqual([
      { id: 'demo:midnight', label: 'Midnight', variant: 'dark' },
    ]);
  });

  it('falls back to the system theme when the plugin is removed but keeps the choice', async () => {
    registerPluginThemes([theme]);
    const { result } = renderHook(() => useTheme(), { wrapper });
    await waitFor(() => expect(result.current.resolvedThemeId).toBe('demo:midnight'));

    act(() => registerPluginThemes([]));

    await waitFor(() => expect(result.current.resolvedThemeId).toBe('light'));
    expect(result.current.userThemePreference).toBe('demo:midnight');
  });

  it('saves a plugin theme selection', async () => {
    registerPluginThemes([theme]);
    const { result } = renderHook(() => useTheme(), { wrapper });
    await waitFor(() => expect(result.current.userThemePreference).toBe('demo:midnight'));

    await act(async () => {
      result.current.setUserThemePreference('demo:midnight');
    });

    expect(setSetting).toHaveBeenCalledWith('useSystemTheme', false);
    expect(setSetting).toHaveBeenCalledWith('theme', 'demo:midnight');
  });
});
