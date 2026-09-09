import React, { createContext, useContext, useEffect, useState, useCallback, useMemo } from 'react';
import { applyThemeTokens, buildMcpHostStyles, themes } from '../theme/theme-tokens';
import type { ThemeId, ThemeVariant } from '../theme/theme-tokens';
import type { McpUiHostStyles } from '@modelcontextprotocol/ext-apps/app-bridge';

type ThemePreference = 'light' | 'dark' | 'aura' | 'system';
type ResolvedTheme = ThemeVariant;

interface ThemeContextValue {
  userThemePreference: ThemePreference;
  setUserThemePreference: (pref: ThemePreference) => void;
  resolvedThemeId: ThemeId;
  resolvedTheme: ResolvedTheme;
  mcpHostStyles: McpUiHostStyles;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

function getSystemTheme(): ResolvedTheme {
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

// Resolve a user preference to a concrete theme id. 'system' picks the light or
// dark built-in from the OS; named themes (light/dark/aura) map to themselves.
function resolveThemeId(preference: ThemePreference): ThemeId {
  if (preference === 'system') {
    return getSystemTheme();
  }
  return preference;
}

function applyThemeToDocument(theme: ResolvedTheme): void {
  const toRemove = theme === 'dark' ? 'light' : 'dark';
  document.documentElement.classList.add(theme);
  document.documentElement.classList.remove(toRemove);
  document.documentElement.style.colorScheme = theme;
}

interface ThemeProviderProps {
  children: React.ReactNode;
}

export function ThemeProvider({ children }: ThemeProviderProps) {
  // Start with light theme to avoid flash, will update once settings load
  const [userThemePreference, setUserThemePreferenceState] = useState<ThemePreference>('light');
  const [resolvedThemeId, setResolvedThemeId] = useState<ThemeId>('light');
  const resolvedTheme = themes[resolvedThemeId].variant;
  const mcpHostStyles = useMemo(() => buildMcpHostStyles(resolvedThemeId), [resolvedThemeId]);

  useEffect(() => {
    async function loadThemeFromSettings() {
      try {
        const [useSystemTheme, savedTheme] = await Promise.all([
          window.electron.getSetting('useSystemTheme'),
          window.electron.getSetting('theme'),
        ]);

        const preference: ThemePreference = useSystemTheme ? 'system' : savedTheme;

        setUserThemePreferenceState(preference);
        setResolvedThemeId(resolveThemeId(preference));
      } catch (error) {
        console.warn('[ThemeContext] Failed to load theme settings:', error);
      }
    }

    loadThemeFromSettings();
  }, []);

  const setUserThemePreference = useCallback(async (preference: ThemePreference) => {
    setUserThemePreferenceState(preference);

    const resolvedId = resolveThemeId(preference);
    setResolvedThemeId(resolvedId);

    // Save to settings
    try {
      if (preference === 'system') {
        await window.electron.setSetting('useSystemTheme', true);
      } else {
        await window.electron.setSetting('useSystemTheme', false);
        await window.electron.setSetting('theme', preference);
      }
    } catch (error) {
      console.warn('[ThemeContext] Failed to save theme settings:', error);
    }

    // Broadcast to other windows via Electron
    window.electron?.broadcastThemeChange({
      mode: themes[resolvedId].variant,
      useSystemTheme: preference === 'system',
      theme: resolvedId,
    });
  }, []);

  // Listen for system theme changes when preference is 'system'
  useEffect(() => {
    if (userThemePreference !== 'system') return;

    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');

    const handleChange = () => {
      setResolvedThemeId(getSystemTheme());
    };

    mediaQuery.addEventListener('change', handleChange);
    return () => mediaQuery.removeEventListener('change', handleChange);
  }, [userThemePreference]);

  // Listen for theme changes from other windows (via Electron IPC)
  useEffect(() => {
    if (!window.electron) return;

    const handleThemeChanged = (_event: unknown, ...args: unknown[]) => {
      const themeData = args[0] as { useSystemTheme: boolean; theme: ThemeId };
      const newPreference: ThemePreference = themeData.useSystemTheme
        ? 'system'
        : themeData.theme;

      setUserThemePreferenceState(newPreference);
      setResolvedThemeId(resolveThemeId(newPreference));

      // Save to settings (don't await, fire and forget)
      if (newPreference === 'system') {
        window.electron.setSetting('useSystemTheme', true);
      } else {
        window.electron.setSetting('useSystemTheme', false);
        window.electron.setSetting('theme', newPreference);
      }
    };

    window.electron.on('theme-changed', handleThemeChanged);
    return () => {
      window.electron.off('theme-changed', handleThemeChanged);
    };
  }, []);

  // Apply theme class and CSS tokens whenever the resolved theme changes
  useEffect(() => {
    applyThemeToDocument(themes[resolvedThemeId].variant);
    applyThemeTokens(resolvedThemeId);
    document.documentElement.dataset.theme = resolvedThemeId;
  }, [resolvedThemeId]);

  const value: ThemeContextValue = {
    userThemePreference,
    setUserThemePreference,
    resolvedThemeId,
    resolvedTheme,
    mcpHostStyles,
  };

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme(): ThemeContextValue {
  const context = useContext(ThemeContext);
  if (!context) {
    throw new Error('useTheme must be used within a ThemeProvider');
  }
  return context;
}
