import { useEffect } from "react";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import { useChatSessionStore } from "@/features/chat/stores/chatSessionStore";
import { USE_DIRECT_ACP } from "@/shared/api/acpFeatureFlag";
import { setNotificationHandler, getClient } from "@/shared/api/acpConnection";
import notificationHandler from "@/shared/api/acpNotificationHandler";

export function useAppStartup() {
  useEffect(() => {
    (async () => {
      if (USE_DIRECT_ACP) {
        try {
          setNotificationHandler(notificationHandler);
          await getClient();
        } catch (err) {
          console.error("Failed to initialize ACP connection:", err);
        }
      }

      const store = useAgentStore.getState();
      const loadPersonas = async () => {
        store.setPersonasLoading(true);
        try {
          const { listPersonas } = await import("@/shared/api/agents");
          const personas = await listPersonas();
          store.setPersonas(personas);
        } catch (err) {
          console.error("Failed to load personas on startup:", err);
        } finally {
          store.setPersonasLoading(false);
        }
      };

      const loadProviders = async () => {
        store.setProvidersLoading(true);
        try {
          const { discoverAcpProviders } = await import("@/shared/api/acp");
          const providers = await discoverAcpProviders();
          store.setProviders(providers);
        } catch (err) {
          console.error("Failed to load ACP providers on startup:", err);
        } finally {
          store.setProvidersLoading(false);
        }
      };

      const loadSessionState = async () => {
        const t0 = performance.now();
        console.log("[perf:startup] loadSessionState start");
        const { loadSessions, setActiveSession } =
          useChatSessionStore.getState();
        await loadSessions();
        console.log(
          `[perf:startup] loadSessions done in ${(performance.now() - t0).toFixed(1)}ms`,
        );
        setActiveSession(null);
      };

      await Promise.allSettled([
        loadPersonas(),
        loadProviders(),
        loadSessionState(),
      ]);
    })();
  }, []);
}
