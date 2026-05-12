import { useCallback } from "react";
import { useAgentStore } from "@/features/agents/stores/agentStore";

export function useCreatePersonaNavigation(navigateToAgents: () => void) {
  return useCallback(() => {
    navigateToAgents();
    const agentStoreState = useAgentStore.getState();
    if (
      agentStoreState.personaEditorOpen &&
      agentStoreState.personaEditorMode === "create" &&
      agentStoreState.editingPersona === null
    ) {
      return;
    }
    agentStoreState.openPersonaEditor(undefined, "create");
  }, [navigateToAgents]);
}
