import type { ChatSessionStore } from "./chatSessionStore";

export const selectSessions = (state: ChatSessionStore) => state.sessions;

export const selectActiveSessionId = (state: ChatSessionStore) =>
  state.activeSessionId;

export const selectHasHydratedSessions = (state: ChatSessionStore) =>
  state.hasHydratedSessions;

export const selectSessionsLoading = (state: ChatSessionStore) =>
  state.isLoading;
