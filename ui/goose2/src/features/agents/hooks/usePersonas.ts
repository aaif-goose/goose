import { useEffect, useCallback, useRef } from "react";
import { useAgentStore } from "../stores/agentStore";
import {
  selectPersonas,
  selectPersonasLoading,
} from "../stores/agentSelectors";
import type {
  CreatePersonaRequest,
  UpdatePersonaRequest,
} from "@/shared/types/agents";
import * as api from "@/shared/api/agents";

const REFRESH_INTERVAL_MS = 60_000;

export function usePersonas() {
  const personas = useAgentStore(selectPersonas);
  const personasLoading = useAgentStore(selectPersonasLoading);
  const setPersonas = useAgentStore((s) => s.setPersonas);
  const addPersona = useAgentStore((s) => s.addPersona);
  const updatePersonaInStore = useAgentStore((s) => s.updatePersona);
  const removePersona = useAgentStore((s) => s.removePersona);
  const setPersonasLoading = useAgentStore((s) => s.setPersonasLoading);
  const refreshTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const loadPersonas = useCallback(async () => {
    setPersonasLoading(true);
    try {
      const personas = await api.listPersonas();
      setPersonas(personas);
    } catch (error) {
      console.error("Failed to load personas:", error);
      // Fall back to empty list - builtins will come from backend
    } finally {
      setPersonasLoading(false);
    }
  }, [setPersonas, setPersonasLoading]);

  const refreshFromDisk = useCallback(async () => {
    try {
      const personas = await api.refreshPersonas();
      setPersonas(personas);
    } catch (error) {
      console.error("Failed to refresh personas from disk:", error);
    }
  }, [setPersonas]);

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

  const createPersona = useCallback(
    async (req: CreatePersonaRequest) => {
      const persona = await api.createPersona(req);
      addPersona(persona);
      return persona;
    },
    [addPersona],
  );

  const updatePersona = useCallback(
    async (id: string, req: UpdatePersonaRequest) => {
      const persona = await api.updatePersona(id, req);
      updatePersonaInStore(id, persona);
      return persona;
    },
    [updatePersonaInStore],
  );

  const deletePersona = useCallback(
    async (id: string) => {
      await api.deletePersona(id);
      removePersona(id);
    },
    [removePersona],
  );

  return {
    personas,
    isLoading: personasLoading,
    createPersona,
    updatePersona,
    deletePersona,
    refresh: loadPersonas,
    refreshFromDisk,
  };
}
