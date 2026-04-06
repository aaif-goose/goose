import { createContext, useContext, useEffect, useState, useMemo } from 'react';
import { getFeatures } from '../api';

interface FeaturesContextValue {
  localInference: boolean;
  codeMode: boolean;
  isLoading: boolean;
}

const FeaturesContext = createContext<FeaturesContextValue | null>(null);

export function FeaturesProvider({ children }: { children: React.ReactNode }) {
  const [features, setFeatures] = useState<Record<string, boolean>>({});
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    (async () => {
      try {
        const response = await getFeatures({ throwOnError: false });
        if (response.data) {
          setFeatures(response.data.features);
        }
      } catch (error) {
        console.warn('[FeaturesContext] Failed to fetch features:', error);
      } finally {
        setIsLoading(false);
      }
    })();
  }, []);

  const value = useMemo<FeaturesContextValue>(
    () => ({
      localInference: features['local-inference'] ?? false,
      codeMode: features['code-mode'] ?? true,
      isLoading,
    }),
    [features, isLoading]
  );

  return <FeaturesContext.Provider value={value}>{children}</FeaturesContext.Provider>;
}

export function useFeatures(): FeaturesContextValue {
  const context = useContext(FeaturesContext);
  if (!context) {
    throw new Error('useFeatures must be used within a FeaturesProvider');
  }
  return context;
}
