import React, {
  createContext,
  ReactNode,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from 'react';

export type NavigationMode = 'push' | 'overlay';
export type NavigationStyle = 'expanded' | 'condensed';
export type NavigationPosition = 'top' | 'bottom' | 'left' | 'right';

export interface NavigationPreferences {
  itemOrder: string[];
  enabledItems: string[];
}

export const DEFAULT_ITEM_ORDER = [
  'home',
  'chat',
  'recipes',
  'skills',
  'apps',
  'scheduler',
  'extensions',
  'settings',
];

export const DEFAULT_ENABLED_ITEMS = [...DEFAULT_ITEM_ORDER];

const RESPONSIVE_BREAKPOINT = 700;

interface NavigationContextValue {
  isNavExpanded: boolean;
  setIsNavExpanded: (expanded: boolean) => void;
  navigationMode: NavigationMode;
  setNavigationMode: (mode: NavigationMode) => void;
  effectiveNavigationMode: NavigationMode;
  navigationStyle: NavigationStyle;
  setNavigationStyle: (style: NavigationStyle) => void;
  effectiveNavigationStyle: NavigationStyle;
  navigationPosition: NavigationPosition;
  setNavigationPosition: (position: NavigationPosition) => void;
  preferences: NavigationPreferences;
  updatePreferences: (prefs: NavigationPreferences) => void;
  isHorizontalNav: boolean;
  isCondensedIconOnly: boolean;
  isOverlayMode: boolean;
  isChatExpanded: boolean;
  setIsChatExpanded: (expanded: boolean) => void;
}

const NavigationContext = createContext<NavigationContextValue | null>(null);

export const useNavigationContext = () => {
  const context = useContext(NavigationContext);
  if (!context) {
    throw new Error('useNavigationContext must be used within NavigationProvider');
  }
  return context;
};

export const useNavigationContextSafe = () => {
  return useContext(NavigationContext);
};

interface NavigationProviderProps {
  children: ReactNode;
}

export const NavigationProvider: React.FC<NavigationProviderProps> = ({ children }) => {
  const [isNavExpanded, setIsNavExpandedState] = useState<boolean>(() => {
    const stored = localStorage.getItem('navigation_expanded');
    return stored !== 'false';
  });

  const [isBelowBreakpoint, setIsBelowBreakpoint] = useState<boolean>(
    () => window.innerWidth < RESPONSIVE_BREAKPOINT
  );

  const [navigationMode, setNavigationModeState] = useState<NavigationMode>(() => {
    const stored = localStorage.getItem('navigation_mode');
    return (stored as NavigationMode) || 'push';
  });

  const [navigationStyle, setNavigationStyleState] = useState<NavigationStyle>(() => {
    const stored = localStorage.getItem('navigation_style');
    return (stored as NavigationStyle) || 'condensed';
  });

  const [navigationPosition, setNavigationPositionState] = useState<NavigationPosition>(() => {
    const stored = localStorage.getItem('navigation_position');
    return (stored as NavigationPosition) || 'left';
  });

  const [preferences, setPreferences] = useState<NavigationPreferences>(() => {
    const stored = localStorage.getItem('navigation_preferences');
    if (stored) {
      try {
        const parsed = JSON.parse(stored);
        // Only backfill truly new default IDs (not previously known to the user).
        // Using itemOrder as the source of truth ensures items the user
        // intentionally disabled stay disabled.
        const newIds = DEFAULT_ITEM_ORDER.filter(
          (id) => !parsed.itemOrder?.includes(id)
        );
        return {
          itemOrder: [...(parsed.itemOrder ?? []), ...newIds],
          enabledItems: [...(parsed.enabledItems ?? []), ...newIds],
        };
      } catch {
        console.error('Failed to parse navigation preferences');
      }
    }
    return {
      itemOrder: DEFAULT_ITEM_ORDER,
      enabledItems: DEFAULT_ENABLED_ITEMS,
    };
  });

  const [isChatExpanded, setIsChatExpandedState] = useState<boolean>(() => {
    const stored = localStorage.getItem('navigation_chat_expanded');
    return stored !== 'false';
  });

  useEffect(() => {
    const mql = window.matchMedia(`(max-width: ${RESPONSIVE_BREAKPOINT - 1}px)`);
    const onChange = () => setIsBelowBreakpoint(window.innerWidth < RESPONSIVE_BREAKPOINT);
    mql.addEventListener('change', onChange);
    setIsBelowBreakpoint(window.innerWidth < RESPONSIVE_BREAKPOINT);
    return () => mql.removeEventListener('change', onChange);
  }, []);

  const setIsNavExpanded = useCallback((expanded: boolean) => {
    setIsNavExpandedState(expanded);
    localStorage.setItem('navigation_expanded', String(expanded));
  }, []);

  const setNavigationMode = useCallback((mode: NavigationMode) => {
    setNavigationModeState(mode);
    localStorage.setItem('navigation_mode', mode);
    window.dispatchEvent(new CustomEvent('navigation-mode-changed', { detail: { mode } }));
  }, []);

  const setNavigationStyle = useCallback((style: NavigationStyle) => {
    setNavigationStyleState(style);
    localStorage.setItem('navigation_style', style);
    window.dispatchEvent(new CustomEvent('navigation-style-changed', { detail: { style } }));
  }, []);

  const setNavigationPosition = useCallback((position: NavigationPosition) => {
    setNavigationPositionState(position);
    localStorage.setItem('navigation_position', position);
    window.dispatchEvent(new CustomEvent('navigation-position-changed', { detail: { position } }));
  }, []);

  const updatePreferences = useCallback((newPrefs: NavigationPreferences) => {
    setPreferences(newPrefs);
    localStorage.setItem('navigation_preferences', JSON.stringify(newPrefs));
    window.dispatchEvent(new CustomEvent('navigation-preferences-updated', { detail: newPrefs }));
  }, []);

  const setIsChatExpanded = useCallback((expanded: boolean) => {
    setIsChatExpandedState(expanded);
    localStorage.setItem('navigation_chat_expanded', String(expanded));
  }, []);

  const isNavExpandedRef = useRef(isNavExpanded);
  useEffect(() => {
    isNavExpandedRef.current = isNavExpanded;
  }, [isNavExpanded]);

  useEffect(() => {
    const handleToggleNavigation = () => {
      setIsNavExpanded(!isNavExpandedRef.current);
    };
    window.electron.on('toggle-navigation', handleToggleNavigation);
    return () => {
      window.electron.off('toggle-navigation', handleToggleNavigation);
    };
  }, [setIsNavExpanded]);

  useEffect(() => {
    const handleModeChange = (e: Event) => setNavigationModeState((e as CustomEvent).detail.mode);
    const handleStyleChange = (e: Event) =>
      setNavigationStyleState((e as CustomEvent).detail.style);
    const handlePositionChange = (e: Event) =>
      setNavigationPositionState((e as CustomEvent).detail.position);
    const handlePrefsChange = (e: Event) => setPreferences((e as CustomEvent).detail);

    window.addEventListener('navigation-mode-changed', handleModeChange);
    window.addEventListener('navigation-style-changed', handleStyleChange);
    window.addEventListener('navigation-position-changed', handlePositionChange);
    window.addEventListener('navigation-preferences-updated', handlePrefsChange);

    return () => {
      window.removeEventListener('navigation-mode-changed', handleModeChange);
      window.removeEventListener('navigation-style-changed', handleStyleChange);
      window.removeEventListener('navigation-position-changed', handlePositionChange);
      window.removeEventListener('navigation-preferences-updated', handlePrefsChange);
    };
  }, []);

  const isHorizontalNav = navigationPosition === 'top' || navigationPosition === 'bottom';
  const effectiveNavigationMode: NavigationMode =
    navigationStyle === 'expanded' && isBelowBreakpoint ? 'overlay' : navigationMode;
  const effectiveNavigationStyle: NavigationStyle =
    navigationMode === 'overlay' ? 'expanded' : navigationStyle;
  const isCondensedIconOnly = !isHorizontalNav && isBelowBreakpoint;
  const isOverlayMode = effectiveNavigationMode === 'overlay';

  const value: NavigationContextValue = {
    isNavExpanded,
    setIsNavExpanded,
    navigationMode,
    setNavigationMode,
    effectiveNavigationMode,
    navigationStyle,
    setNavigationStyle,
    effectiveNavigationStyle,
    navigationPosition,
    setNavigationPosition,
    preferences,
    updatePreferences,
    isHorizontalNav,
    isCondensedIconOnly,
    isOverlayMode,
    isChatExpanded,
    setIsChatExpanded,
  };

  return <NavigationContext.Provider value={value}>{children}</NavigationContext.Provider>;
};
