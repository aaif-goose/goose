import { useEffect, useCallback, useRef } from "react";
import { useAgentStore } from "../stores/agentStore";
import type {
  CreatePersonaRequest,
  UpdatePersonaRequest,
} from "@/shared/types/agents";
import * as api from "@/shared/api/agents";

const REFRESH_INTERVAL_MS = 60_000;

export function usePersonas() {
  const store = useAgentStore();
  const refreshTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // biome-ignore lint/correctness/useExhaustiveDependencies: store is stable and should not trigger re-creation
  const loadPersonas = useCallback(async () => {
    store.setPersonasLoading(true);
    try {
      const personas = await api.listPersonas();
      store.setPersonas(personas);
    } catch (error) {
      console.error("Failed to load personas:", error);
      // Fall back to empty list - builtins will come from backend
    } finally {
      store.setPersonasLoading(false);
    }
  }, []);

  // biome-ignore lint/correctness/useExhaustiveDependencies: store is stable and should not trigger re-creation
  const refreshFromDisk = useCallback(async () => {
    try {
      const personas = await api.refreshPersonas();
      store.setPersonas(personas);
    } catch (error) {
      console.error("Failed to refresh personas from disk:", error);
    }
  }, []);

  useEffect(() => {
    loadPersonas();
  }, [loadPersonas]);

  // Periodic refresh every 60s and on window focus
  useEffect(() => {
    refreshTimerRef.current = setInterval(refreshFromDisk, REFRESH_INTERVAL_MS);

    const handleFocus = () => {
      refreshFromDisk();
    };
    window.addEventListener("focus", handleFocus);

    return () => {
      if (refreshTimerRef.current) {
        clearInterval(refreshTimerRef.current);
      }
      window.removeEventListener("focus", handleFocus);
    };
  }, [refreshFromDisk]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: store is stable and should not trigger re-creation
  const createPersona = useCallback(async (req: CreatePersonaRequest) => {
    const persona = await api.createPersona(req);
    store.addPersona(persona);
    return persona;
  }, []);

  // biome-ignore lint/correctness/useExhaustiveDependencies: store is stable and should not trigger re-creation
  const updatePersona = useCallback(
    async (id: string, req: UpdatePersonaRequest) => {
      const persona = await api.updatePersona(id, req);
      store.updatePersona(id, persona);
      return persona;
    },
    [],
  );

  // biome-ignore lint/correctness/useExhaustiveDependencies: store is stable and should not trigger re-creation
  const deletePersona = useCallback(async (id: string) => {
    await api.deletePersona(id);
    store.removePersona(id);
  }, []);

  return {
    personas: store.personas,
    isLoading: store.personasLoading,
    createPersona,
    updatePersona,
    deletePersona,
    refresh: loadPersonas,
    refreshFromDisk,
  };
}
