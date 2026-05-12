import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act, waitFor } from "@testing-library/react";
import { useAgentStore } from "../../stores/agentStore";
import type { Persona } from "@/shared/types/agents";

// ── mocks ────────────────────────────────────────────────────────────

vi.mock("@/shared/api/agents", () => ({
  listPersonas: vi.fn().mockResolvedValue([]),
  createPersona: vi.fn().mockResolvedValue({
    id: "new-id",
    displayName: "Test",
    systemPrompt: "You are helpful.",
    isBuiltin: false,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
  }),
  updatePersona: vi.fn().mockResolvedValue({
    id: "test-id",
    displayName: "Updated",
    systemPrompt: "Updated prompt",
    isBuiltin: false,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
  }),
  deletePersona: vi.fn().mockResolvedValue(undefined),
  refreshPersonas: vi.fn().mockResolvedValue([]),
}));

// Import the mocked module so we can inspect/adjust calls
import * as api from "@/shared/api/agents";

// Import the hook after mocks are set up
import { usePersonas } from "../usePersonas";

// ── helpers ──────────────────────────────────────────────────────────

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

// ── tests ────────────────────────────────────────────────────────────

describe("usePersonas", () => {
  beforeEach(() => {
    // Re-establish default mock implementations (clearAllMocks would wipe them)
    vi.mocked(api.listPersonas).mockReset().mockResolvedValue([]);
    vi.mocked(api.createPersona).mockReset().mockResolvedValue({
      id: "new-id",
      displayName: "Test",
      systemPrompt: "You are helpful.",
      isBuiltin: false,
      createdAt: "2026-01-01T00:00:00Z",
      updatedAt: "2026-01-01T00:00:00Z",
    });
    vi.mocked(api.updatePersona).mockReset().mockResolvedValue({
      id: "test-id",
      displayName: "Updated",
      systemPrompt: "Updated prompt",
      isBuiltin: false,
      createdAt: "2026-01-01T00:00:00Z",
      updatedAt: "2026-01-01T00:00:00Z",
    });
    vi.mocked(api.deletePersona).mockReset().mockResolvedValue(undefined);
    vi.mocked(api.refreshPersonas).mockReset().mockResolvedValue([]);

    useAgentStore.setState({
      personas: [],
      personasLoading: false,
      agents: [],
      agentsLoading: false,
      activeAgentId: null,
      isLoading: false,
      personaEditorOpen: false,
      editingPersona: null,
      personaEditorMode: "create",
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  // ── loading ────────────────────────────────────────────────────────

  describe("loading personas", () => {
    it("loads personas on mount via listPersonas()", async () => {
      const personas = [makePersona({ id: "p1" }), makePersona({ id: "p2" })];
      vi.mocked(api.listPersonas).mockResolvedValueOnce(personas);

      const { result } = renderHook(() => usePersonas());

      await waitFor(() => {
        expect(api.listPersonas).toHaveBeenCalledTimes(1);
      });

      await waitFor(() => {
        expect(result.current.personas).toEqual(personas);
      });
    });

    it("sets loading state correctly", async () => {
      // Create a deferred promise to control timing
      let resolveList!: (value: Persona[]) => void;
      vi.mocked(api.listPersonas).mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveList = resolve;
          }),
      );

      const { result } = renderHook(() => usePersonas());

      // Should be loading while the API call is in flight
      await waitFor(() => {
        expect(result.current.isLoading).toBe(true);
      });

      // Resolve the API call
      await act(async () => {
        resolveList([]);
      });

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });
    });
  });

  // ── CRUD operations ────────────────────────────────────────────────

  describe("CRUD operations", () => {
    it("createPersona calls API and adds to store", async () => {
      const newPersona = {
        id: "new-id",
        displayName: "Test",
        systemPrompt: "You are helpful.",
        isBuiltin: false,
        createdAt: "2026-01-01T00:00:00Z",
        updatedAt: "2026-01-01T00:00:00Z",
      };
      vi.mocked(api.createPersona).mockResolvedValueOnce(newPersona);

      const { result } = renderHook(() => usePersonas());

      // Wait for initial load to fully complete
      await waitFor(() => {
        expect(api.listPersonas).toHaveBeenCalledTimes(1);
        expect(result.current.isLoading).toBe(false);
      });

      let created: Persona | undefined;
      await act(async () => {
        created = await result.current.createPersona({
          displayName: "Test",
          systemPrompt: "You are helpful.",
        });
      });

      expect(api.createPersona).toHaveBeenCalledWith({
        displayName: "Test",
        systemPrompt: "You are helpful.",
      });
      expect(created).toEqual(newPersona);
      expect(result.current.personas).toContainEqual(newPersona);
    });

    it("updatePersona calls API and updates store", async () => {
      const existing = makePersona({ id: "test-id", displayName: "Old" });
      // Return existing persona from initial load so the store has it
      vi.mocked(api.listPersonas).mockResolvedValueOnce([existing]);

      const updated = {
        id: "test-id",
        displayName: "Updated",
        systemPrompt: "Updated prompt",
        isBuiltin: false,
        createdAt: "2026-01-01T00:00:00Z",
        updatedAt: "2026-01-01T00:00:00Z",
      };
      vi.mocked(api.updatePersona).mockResolvedValueOnce(updated);

      const { result } = renderHook(() => usePersonas());

      // Wait for initial load to populate store
      await waitFor(() => {
        expect(result.current.personas).toHaveLength(1);
      });

      await act(async () => {
        await result.current.updatePersona("test-id", {
          displayName: "Updated",
        });
      });

      expect(api.updatePersona).toHaveBeenCalledWith("test-id", {
        displayName: "Updated",
      });
      expect(
        result.current.personas.find((p) => p.id === "test-id")?.displayName,
      ).toBe("Updated");
    });

    it("deletePersona calls API and removes from store", async () => {
      const existing = makePersona({ id: "del-id" });
      // Return existing persona from initial load so the store has it
      vi.mocked(api.listPersonas).mockResolvedValueOnce([existing]);

      const { result } = renderHook(() => usePersonas());

      // Wait for initial load to populate store
      await waitFor(() => {
        expect(result.current.personas).toHaveLength(1);
      });

      await act(async () => {
        await result.current.deletePersona("del-id");
      });

      expect(api.deletePersona).toHaveBeenCalledWith("del-id");
      expect(
        result.current.personas.find((p) => p.id === "del-id"),
      ).toBeUndefined();
    });
  });

  // ── refresh ────────────────────────────────────────────────────────

  describe("refresh", () => {
    it("refreshFromDisk calls refreshPersonas() API", async () => {
      const refreshed = [makePersona({ id: "refreshed-1" })];
      vi.mocked(api.refreshPersonas).mockResolvedValueOnce(refreshed);

      const { result } = renderHook(() => usePersonas());

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });

      await act(async () => {
        await result.current.refreshFromDisk();
      });

      expect(api.refreshPersonas).toHaveBeenCalled();
      expect(result.current.personas).toEqual(refreshed);
    });
  });
});
