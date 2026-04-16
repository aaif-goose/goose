import { useState, useEffect, useCallback } from "react";
import {
  checkAgentInstalled,
  checkAgentAuth,
} from "@/features/providers/api/agentSetup";
import {
  getAgentProviders,
  getCatalogEntry,
} from "@/features/providers/providerCatalog";

interface UseAgentProviderStatusReturn {
  readyAgentIds: Set<string>;
  loading: boolean;
  refresh: () => Promise<void>;
}

async function checkAgentProviderReady(providerId: string): Promise<boolean> {
  const provider = getCatalogEntry(providerId);
  if (!provider || provider.category !== "agent") {
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

    if (provider.authStatusCommand) {
      return checkAgentAuth(provider.id);
    }

    if (provider.authCommand) {
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

export function useAgentProviderStatus(): UseAgentProviderStatusReturn {
  const [readyAgentIds, setReadyAgentIds] =
    useState<Set<string>>(INITIAL_READY_AGENTS);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    const agentIds = getAgentProviders().map((provider) => provider.id);
    let remaining = agentIds.length;

    for (const agentId of agentIds) {
      checkAgentProviderReady(agentId)
        .then((isReady) => {
          if (!cancelled && isReady) {
            setReadyAgentIds((current) => {
              if (current.has(agentId)) {
                return current;
              }

              const next = new Set(current);
              next.add(agentId);
              return next;
            });
          }
        })
        .finally(() => {
          remaining -= 1;
          if (!cancelled && remaining === 0) {
            setLoading(false);
          }
        });
    }

    return () => {
      cancelled = true;
    };
  }, []);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const readiness = await Promise.all(
        getAgentProviders().map(async (provider) => ({
          id: provider.id,
          isReady: await checkAgentProviderReady(provider.id),
        })),
      );
      const readyIds = readiness
        .filter((provider) => provider.isReady)
        .map((provider) => provider.id);
      setReadyAgentIds(new Set(["goose", ...readyIds]));
    } finally {
      setLoading(false);
    }
  }, []);

  return {
    readyAgentIds,
    loading,
    refresh,
  };
}
