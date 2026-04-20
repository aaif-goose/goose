import { useEffect } from "react";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import { useChatSessionStore } from "@/features/chat/stores/chatSessionStore";
import { useProviderInventoryStore } from "@/features/providers/stores/providerInventoryStore";
import { setNotificationHandler, getClient } from "@/shared/api/acpConnection";
import notificationHandler from "@/shared/api/acpNotificationHandler";
import { perfLog } from "@/shared/lib/perfLog";

const INVENTORY_POLL_DELAYS_MS = [250, 500, 750, 1000, 1500, 2000];

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

export function useAppStartup() {
  useEffect(() => {
    (async () => {
      const tStartup = performance.now();
      perfLog("[perf:startup] useAppStartup begin");
      try {
        const tConn = performance.now();
        setNotificationHandler(notificationHandler);
        await getClient();
        perfLog(
          `[perf:startup] ACP getClient ready in ${(performance.now() - tConn).toFixed(1)}ms`,
        );
      } catch (err) {
        console.error("Failed to initialize ACP connection:", err);
      }

      const store = useAgentStore.getState();
      const inventoryStore = useProviderInventoryStore.getState();
      const loadPersonas = async () => {
        const t0 = performance.now();
        store.setPersonasLoading(true);
        try {
          const { listPersonas } = await import("@/shared/api/agents");
          const personas = await listPersonas();
          store.setPersonas(personas);
          perfLog(
            `[perf:startup] loadPersonas done in ${(performance.now() - t0).toFixed(1)}ms (n=${personas.length})`,
          );
        } catch (err) {
          console.error("Failed to load personas on startup:", err);
        } finally {
          store.setPersonasLoading(false);
        }
      };

      const loadProviders = async () => {
        const t0 = performance.now();
        store.setProvidersLoading(true);
        try {
          const { discoverAcpProviders } = await import("@/shared/api/acp");
          const providers = await discoverAcpProviders();
          store.setProviders(providers);
          perfLog(
            `[perf:startup] loadProviders done in ${(performance.now() - t0).toFixed(1)}ms (n=${providers.length})`,
          );
        } catch (err) {
          console.error("Failed to load ACP providers on startup:", err);
        } finally {
          store.setProvidersLoading(false);
        }
      };

      const loadProviderInventory = async () => {
        const t0 = performance.now();
        inventoryStore.setLoading(true);
        try {
          const { getProviderInventory } = await import(
            "@/features/providers/api/inventory"
          );
          const entries = await getProviderInventory();
          inventoryStore.setEntries(entries);
          perfLog(
            `[perf:startup] loadProviderInventory done in ${(performance.now() - t0).toFixed(1)}ms (n=${entries.length})`,
          );
          return entries;
        } catch (err) {
          console.error("Failed to load provider inventory on startup:", err);
          return [];
        } finally {
          inventoryStore.setLoading(false);
        }
      };

      const refreshConfiguredProviderInventory = async (
        initialEntries?: Awaited<ReturnType<typeof loadProviderInventory>>,
      ) => {
        try {
          const entries =
            initialEntries && initialEntries.length > 0
              ? initialEntries
              : await (async () => {
                  const { getProviderInventory } = await import(
                    "@/features/providers/api/inventory"
                  );
                  return getProviderInventory();
                })();
          const configuredProviderIds = entries
            .filter((entry) => entry.configured)
            .map((entry) => entry.providerId);
          if (configuredProviderIds.length === 0) {
            return;
          }

          const { getProviderInventory, refreshProviderInventory } =
            await import("@/features/providers/api/inventory");
          const refresh = await refreshProviderInventory(configuredProviderIds);
          if (refresh.started.length === 0) {
            return;
          }

          inventoryStore.mergeEntries(
            await getProviderInventory(refresh.started),
          );

          for (const delayMs of INVENTORY_POLL_DELAYS_MS) {
            await sleep(delayMs);
            const refreshedEntries = await getProviderInventory(
              refresh.started,
            );
            inventoryStore.mergeEntries(refreshedEntries);
            if (refreshedEntries.every((entry) => !entry.refreshing)) {
              return;
            }
          }
        } catch (err) {
          console.error(
            "Failed to refresh provider inventory on startup:",
            err,
          );
        }
      };

      const loadSessionState = async () => {
        const t0 = performance.now();
        perfLog("[perf:startup] loadSessionState start");
        const { loadSessions, setActiveSession } =
          useChatSessionStore.getState();
        await loadSessions();
        perfLog(
          `[perf:startup] loadSessions done in ${(performance.now() - t0).toFixed(1)}ms`,
        );
        setActiveSession(null);
      };

      const inventoryLoad = loadProviderInventory();

      await Promise.allSettled([
        loadPersonas(),
        loadProviders(),
        inventoryLoad,
        loadSessionState(),
      ]);
      void inventoryLoad.then((entries) =>
        refreshConfiguredProviderInventory(entries),
      );
      perfLog(
        `[perf:startup] useAppStartup complete in ${(performance.now() - tStartup).toFixed(1)}ms`,
      );
    })();
  }, []);
}
