import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { importPersonas, refreshPersonas } from "../agents";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const mockedInvoke = vi.mocked(invoke);

describe("agents API", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  // ── importPersonas ───────────────────────────────────────────────────

  it("importPersonas invokes correct Tauri command with bytes and filename", async () => {
    const mockPersonas = [
      {
        id: "imported-1",
        displayName: "Imported",
        systemPrompt: "Hello",
        isBuiltin: false,
        createdAt: "2026-01-01T00:00:00Z",
        updatedAt: "2026-01-01T00:00:00Z",
      },
    ];
    mockedInvoke.mockResolvedValue(mockPersonas);

    const fileBytes = [0x7b, 0x7d]; // "{}"
    const result = await importPersonas(fileBytes, "agent.md");

    expect(mockedInvoke).toHaveBeenCalledWith("import_personas", {
      fileBytes,
      fileName: "agent.md",
    });
    expect(result).toEqual(mockPersonas);
  });

  // ── refreshPersonas ──────────────────────────────────────────────────

  it("refreshPersonas invokes correct Tauri command", async () => {
    const mockPersonas = [
      {
        id: "p1",
        displayName: "Refreshed",
        systemPrompt: "Prompt",
        isBuiltin: false,
        createdAt: "2026-01-01T00:00:00Z",
        updatedAt: "2026-01-01T00:00:00Z",
      },
    ];
    mockedInvoke.mockResolvedValue(mockPersonas);

    const result = await refreshPersonas();

    expect(mockedInvoke).toHaveBeenCalledWith("refresh_personas");
    expect(result).toEqual(mockPersonas);
  });
});
