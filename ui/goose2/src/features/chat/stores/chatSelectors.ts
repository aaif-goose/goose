import type { ChatStore } from "./chatStore";

export const selectMessagesBySession = (state: ChatStore) =>
  state.messagesBySession;

export const selectSessionStateById = (state: ChatStore) =>
  state.sessionStateById;
