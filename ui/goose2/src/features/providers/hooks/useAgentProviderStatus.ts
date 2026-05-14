import { useState, useEffect, useCallback } from "react";
import {
  checkAgentInstalled,
  checkAgentAuth,
} from "@/features/providers/api/agentSetup";
import { getAgentProvidersFromEntries } from "@/features/providers/providerCatalog";
import { useProviderCatalogStore } from "@/features/providers/stores/providerCatalogStore";
import type { ProviderCatalogEntry } from "@/shared/types/providers";

interface UseAgentProviderStatusReturn {
  readyAgentIds: Set<string>;
  loading: boolean;
  refresh: () => Promise<void>;
}

async function checkAgentProviderReady(
  provider: ProviderCatalogEntry,
): Promise<boolean> {
  if (provider.category !== "agent") {
    return false;
  }

  if (provider.setupMethod === "none") {
    return true;
  }

  if (!provider.binaryName) {
    return false;
  }

  try {
    const installed = await checkAgentInstalled(provider.id);
    if (!installed) {
      return false;
    }

    if (provider.supportsAuthStatus) {
      return checkAgentAuth(provider.id);
    }

    if (provider.supportsAuth) {
      return (
        localStorage.getItem(`agent-provider-auth:${provider.id}`) === "true"
      );
    }

    return true;
  } catch {
    return false;
  }
}

const INITIAL_READY_AGENTS = new Set<string>(["goose"]);

async function checkReadyAgentIds(
  agents: ProviderCatalogEntry[],
): Promise<Set<string>> {
  const readiness = await Promise.all(
    agents.map(async (provider) => ({
      id: provider.id,
      isReady: await checkAgentProviderReady(provider),
    })),
  );
  const readyIds = readiness
    .filter((provider) => provider.isReady)
    .map((provider) => provider.id);
  return new Set(["goose", ...readyIds]);
}

export function useAgentProviderStatus(): UseAgentProviderStatusReturn {
  const catalogEntries = useProviderCatalogStore((state) => state.entries);
  const [readyAgentIds, setReadyAgentIds] =
    useState<Set<string>>(INITIAL_READY_AGENTS);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    const agents = getAgentProvidersFromEntries(catalogEntries);
    setLoading(true);
    checkReadyAgentIds(agents)
      .then((nextReadyAgentIds) => {
        if (!cancelled) {
          setReadyAgentIds(nextReadyAgentIds);
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [catalogEntries]);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const agents = getAgentProvidersFromEntries(catalogEntries);
      setReadyAgentIds(await checkReadyAgentIds(agents));
    } finally {
      setLoading(false);
    }
  }, [catalogEntries]);

  return {
    readyAgentIds,
    loading,
    refresh,
  };
}
