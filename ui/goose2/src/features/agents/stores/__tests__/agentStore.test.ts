import { describe, it, expect, beforeEach } from "vitest";
import { useAgentStore } from "../agentStore";
import type { Persona, Agent } from "@/shared/types/agents";

// ── fixtures ──────────────────────────────────────────────────────────

function makePersona(overrides: Partial<Persona> = {}): Persona {
  return {
    id: crypto.randomUUID(),
    displayName: "Test Persona",
    systemPrompt: "You are helpful.",
    isBuiltin: false,
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
    ...overrides,
  };
}

function makeAgent(overrides: Partial<Agent> = {}): Agent {
  return {
    id: crypto.randomUUID(),
    name: "Test Agent",
    provider: "goose",
    model: "claude-sonnet-4",
    connectionType: "builtin",
    status: "online",
    isBuiltin: false,
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
    ...overrides,
  };
}

// ── tests ─────────────────────────────────────────────────────────────

describe("agentStore", () => {
  beforeEach(() => {
    useAgentStore.setState({
      personas: [],
      personasLoading: false,
      agents: [],
      agentsLoading: false,
      activeAgentId: null,
      isLoading: false,
      personaEditorOpen: false,
      editingPersona: null,
    });
  });

  // ── initial state ─────────────────────────────────────────────────

  it("has empty personas and agents initially", () => {
    const state = useAgentStore.getState();
    expect(state.personas).toEqual([]);
    expect(state.agents).toEqual([]);
  });

  // ── persona CRUD ──────────────────────────────────────────────────

  it("setPersonas replaces personas", () => {
    const p1 = makePersona({ id: "p1" });
    const p2 = makePersona({ id: "p2" });
    useAgentStore.getState().setPersonas([p1, p2]);
    expect(useAgentStore.getState().personas).toEqual([p1, p2]);
  });

  it("addPersona appends a persona", () => {
    const p = makePersona();
    useAgentStore.getState().addPersona(p);
    expect(useAgentStore.getState().personas).toHaveLength(1);
    expect(useAgentStore.getState().personas[0].id).toBe(p.id);
  });

  it("updatePersona updates the correct persona", () => {
    const p = makePersona({ id: "up1", displayName: "Old" });
    useAgentStore.getState().setPersonas([p]);
    useAgentStore.getState().updatePersona("up1", { displayName: "New" });
    expect(useAgentStore.getState().personas[0].displayName).toBe("New");
  });

  it("removePersona removes the correct persona", () => {
    const p1 = makePersona({ id: "keep" });
    const p2 = makePersona({ id: "remove" });
    useAgentStore.getState().setPersonas([p1, p2]);
    useAgentStore.getState().removePersona("remove");
    expect(useAgentStore.getState().personas).toHaveLength(1);
    expect(useAgentStore.getState().personas[0].id).toBe("keep");
  });

  // ── agent CRUD ────────────────────────────────────────────────────

  it("setAgents replaces agents", () => {
    const a = makeAgent();
    useAgentStore.getState().setAgents([a]);
    expect(useAgentStore.getState().agents).toEqual([a]);
  });

  it("addAgent appends an agent", () => {
    const a = makeAgent();
    useAgentStore.getState().addAgent(a);
    expect(useAgentStore.getState().agents).toHaveLength(1);
  });

  it("updateAgent updates the correct agent", () => {
    const a = makeAgent({ id: "ua1", name: "Old" });
    useAgentStore.getState().setAgents([a]);
    useAgentStore.getState().updateAgent("ua1", { name: "New" });
    expect(useAgentStore.getState().agents[0].name).toBe("New");
  });

  it("removeAgent removes the correct agent", () => {
    const a1 = makeAgent({ id: "keep" });
    const a2 = makeAgent({ id: "remove" });
    useAgentStore.getState().setAgents([a1, a2]);
    useAgentStore.getState().removeAgent("remove");
    expect(useAgentStore.getState().agents).toHaveLength(1);
    expect(useAgentStore.getState().agents[0].id).toBe("keep");
  });

  // ── active agent ──────────────────────────────────────────────────

  it("setActiveAgent updates activeAgentId", () => {
    useAgentStore.getState().setActiveAgent("a1");
    expect(useAgentStore.getState().activeAgentId).toBe("a1");
  });

  it("getActiveAgent returns correct agent or null", () => {
    expect(useAgentStore.getState().getActiveAgent()).toBeNull();

    const a = makeAgent({ id: "active-1" });
    useAgentStore.getState().setAgents([a]);
    useAgentStore.getState().setActiveAgent("active-1");
    expect(useAgentStore.getState().getActiveAgent()).toEqual(a);
  });

  // ── persona editor ────────────────────────────────────────────────

  it("openPersonaEditor sets editing state", () => {
    const p = makePersona();
    useAgentStore.getState().openPersonaEditor(p);
    expect(useAgentStore.getState().personaEditorOpen).toBe(true);
    expect(useAgentStore.getState().editingPersona).toEqual(p);
  });

  it("openPersonaEditor without persona sets editingPersona to null", () => {
    useAgentStore.getState().openPersonaEditor();
    expect(useAgentStore.getState().personaEditorOpen).toBe(true);
    expect(useAgentStore.getState().editingPersona).toBeNull();
  });

  it("closePersonaEditor clears editing state", () => {
    useAgentStore.getState().openPersonaEditor(makePersona());
    useAgentStore.getState().closePersonaEditor();
    expect(useAgentStore.getState().personaEditorOpen).toBe(false);
    expect(useAgentStore.getState().editingPersona).toBeNull();
  });

  // ── helpers ───────────────────────────────────────────────────────

  it("getPersonaById returns correct persona", () => {
    const p = makePersona({ id: "find-me" });
    useAgentStore.getState().setPersonas([p]);
    expect(useAgentStore.getState().getPersonaById("find-me")).toEqual(p);
    expect(useAgentStore.getState().getPersonaById("nope")).toBeUndefined();
  });

  it("getAgentsByPersona filters correctly", () => {
    const a1 = makeAgent({ id: "a1", personaId: "p1" });
    const a2 = makeAgent({ id: "a2", personaId: "p2" });
    const a3 = makeAgent({ id: "a3", personaId: "p1" });
    useAgentStore.getState().setAgents([a1, a2, a3]);
    const result = useAgentStore.getState().getAgentsByPersona("p1");
    expect(result).toHaveLength(2);
    expect(result.map((a) => a.id).sort()).toEqual(["a1", "a3"]);
  });

  it("getBuiltinPersonas returns only builtins", () => {
    useAgentStore
      .getState()
      .setPersonas([
        makePersona({ id: "b", isBuiltin: true }),
        makePersona({ id: "c", isBuiltin: false }),
      ]);
    const builtins = useAgentStore.getState().getBuiltinPersonas();
    expect(builtins).toHaveLength(1);
    expect(builtins[0].id).toBe("b");
  });

  it("getCustomPersonas returns only non-builtins", () => {
    useAgentStore
      .getState()
      .setPersonas([
        makePersona({ id: "b", isBuiltin: true }),
        makePersona({ id: "c", isBuiltin: false }),
      ]);
    const custom = useAgentStore.getState().getCustomPersonas();
    expect(custom).toHaveLength(1);
    expect(custom[0].id).toBe("c");
  });
});
