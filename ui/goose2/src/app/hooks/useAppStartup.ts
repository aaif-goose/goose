import { useEffect } from "react";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import { useChatSessionStore } from "@/features/chat/stores/chatSessionStore";
import { setNotificationHandler, getClient } from "@/shared/api/acpConnection";
import notificationHandler from "@/shared/api/acpNotificationHandler";
import { perfLog } from "@/shared/lib/perfLog";

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

      await Promise.allSettled([
        loadPersonas(),
        loadProviders(),
        loadSessionState(),
      ]);
      perfLog(
        `[perf:startup] useAppStartup complete in ${(performance.now() - tStartup).toFixed(1)}ms`,
      );
    })();
  }, []);
}
