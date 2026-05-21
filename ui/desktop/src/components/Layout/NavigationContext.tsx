import React, {
  createContext,
  ReactNode,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from 'react';

interface NavigationContextValue {
  isNavExpanded: boolean;
  setIsNavExpanded: (expanded: boolean) => void;
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

  const setIsNavExpanded = useCallback((expanded: boolean) => {
    setIsNavExpandedState(expanded);
    localStorage.setItem('navigation_expanded', String(expanded));
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

  const value: NavigationContextValue = {
    isNavExpanded,
    setIsNavExpanded,
  };

  return <NavigationContext.Provider value={value}>{children}</NavigationContext.Provider>;
};
