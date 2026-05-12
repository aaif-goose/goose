import type { AgentStore } from "./agentStore";

export const selectPersonas = (state: AgentStore) => state.personas;

export const selectPersonasLoading = (state: AgentStore) =>
  state.personasLoading;

export const selectSelectedProvider = (state: AgentStore) =>
  state.selectedProvider;
