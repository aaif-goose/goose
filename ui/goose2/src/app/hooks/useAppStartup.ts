import { useEffect } from "react";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import { useChatSessionStore } from "@/features/chat/stores/chatSessionStore";
import { useProviderInventoryStore } from "@/features/providers/stores/providerInventoryStore";
import { discoverAcpProvidersFromEntries } from "@/shared/api/acp";
import { setNotificationHandler, getClient } from "@/shared/api/acpConnection";
import notificationHandler from "@/shared/api/acpNotificationHandler";
import { perfLog } from "@/shared/lib/perfLog";
import { parseProviderAllowlist } from "@/features/providers/distroProviderConstraints";
import { getModelProviders } from "@/features/providers/providerCatalog";
import { useDistroStore } from "@/features/settings/stores/distroStore";

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
      const distroStore = useDistroStore.getState();
      const loadDistroBundle = async () => {
        try {
          const { getDistroBundle } = await import("@/shared/api/distro");
          const manifest = await getDistroBundle();
          distroStore.setManifest(manifest);
        } catch (err) {
          console.error("Failed to load distro bundle on startup:", err);
          distroStore.setManifest({ present: false });
        }
      };

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

      const loadProvidersAndInventory = async () => {
        const t0 = performance.now();
        store.setProvidersLoading(true);
        inventoryStore.setLoading(true);
        try {
          const { getProviderInventory } = await import(
            "@/features/providers/api/inventory"
          );
          const entries = await getProviderInventory();

          // Populate inventory store
          inventoryStore.setEntries(entries);

          // Derive ACP providers from the same response
          const providers = discoverAcpProvidersFromEntries(entries);
          const providerAllowlist = parseProviderAllowlist(
            useDistroStore.getState().manifest,
          );
          if (!providerAllowlist) {
            store.setProviders(providers);
          } else {
            const hasAllowedModelProvider = getModelProviders().some(
              (provider) => providerAllowlist.has(provider.id),
            );
            store.setProviders(
              providers.filter(
                (provider) =>
                  provider.id !== "goose" || hasAllowedModelProvider,
              ),
            );
          }

          perfLog(
            `[perf:startup] loadProvidersAndInventory done in ${(performance.now() - t0).toFixed(1)}ms (entries=${entries.length}, providers=${providers.length})`,
          );
          return entries;
        } catch (err) {
          console.error(
            "Failed to load providers and inventory on startup:",
            err,
          );
          return [];
        } finally {
          store.setProvidersLoading(false);
          inventoryStore.setLoading(false);
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

      await loadDistroBundle();

      const providersAndInventoryLoad = loadProvidersAndInventory();

      await Promise.allSettled([
        loadPersonas(),
        providersAndInventoryLoad,
        loadSessionState(),
      ]);
      void providersAndInventoryLoad.then(async (entries) => {
        try {
          const { backgroundRefreshInventory } = await import(
            "@/features/providers/api/inventory"
          );
          await backgroundRefreshInventory(inventoryStore, entries);
        } catch (err) {
          console.error(
            "Failed to refresh provider inventory on startup:",
            err,
          );
        }
      });
      perfLog(
        `[perf:startup] useAppStartup complete in ${(performance.now() - tStartup).toFixed(1)}ms`,
      );
    })();
  }, []);
}
