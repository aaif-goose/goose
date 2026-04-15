import { useCallback } from "react";
import { useAgentStore } from "../stores/agentStore";

export function useProviderSelection() {
  const providers = useAgentStore((s) => s.providers);
  const providersLoading = useAgentStore((s) => s.providersLoading);
  const selectedProvider = useAgentStore((s) => s.selectedProvider);
  const storeSetSelectedProvider = useAgentStore((s) => s.setSelectedProvider);

  const setSelectedProvider = useCallback(
    (providerId: string) => {
      storeSetSelectedProvider(providerId, true);
    },
    [storeSetSelectedProvider],
  );

  const setSelectedProviderWithoutPersist = useCallback(
    (providerId: string) => {
      storeSetSelectedProvider(providerId, false);
    },
    [storeSetSelectedProvider],
  );

  return {
    providers,
    providersLoading,
    selectedProvider,
    setSelectedProvider,
    setSelectedProviderWithoutPersist,
  };
}
