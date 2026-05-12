import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  createPersona,
  deletePersona,
  exportPersona,
  importPersonas,
  listPersonas,
  refreshPersonas,
  updatePersona,
} from "../agents";

const mockGooseSourcesCreate = vi.fn();
const mockGooseSourcesDelete = vi.fn();
const mockGooseSourcesExport = vi.fn();
const mockGooseSourcesImport = vi.fn();
const mockGooseSourcesList = vi.fn();
const mockGooseSourcesUpdate = vi.fn();

vi.mock("@/shared/api/acpConnection", () => ({
  getClient: async () => ({
    goose: {
      GooseSourcesCreate: (...args: unknown[]) =>
        mockGooseSourcesCreate(...args),
      GooseSourcesDelete: (...args: unknown[]) =>
        mockGooseSourcesDelete(...args),
      GooseSourcesExport: (...args: unknown[]) =>
        mockGooseSourcesExport(...args),
      GooseSourcesImport: (...args: unknown[]) =>
        mockGooseSourcesImport(...args),
      GooseSourcesList: (...args: unknown[]) => mockGooseSourcesList(...args),
      GooseSourcesUpdate: (...args: unknown[]) =>
        mockGooseSourcesUpdate(...args),
    },
  }),
}));

const source = {
  type: "agent",
  name: "Scout",
  description: "Agent",
  content: "Research carefully.",
  path: "/Users/test/.agents/agents/scout.md",
  global: true,
  writable: true,
  properties: {
    provider: "goose",
    model: "claude-sonnet-4",
    avatar: "file:///Users/test/.goose/avatars/agents/scout.png",
  },
};

describe("agents API", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("listPersonas maps agent sources to personas", async () => {
    mockGooseSourcesList.mockResolvedValue({ sources: [source] });

    const result = await listPersonas();

    expect(mockGooseSourcesList).toHaveBeenCalledWith({ type: "agent" });
    expect(result).toEqual([
      {
        id: source.path,
        displayName: "Scout",
        avatar: { type: "url", value: source.properties.avatar },
        systemPrompt: "Research carefully.",
        provider: "goose",
        model: "claude-sonnet-4",
        isBuiltin: false,
        isFromDisk: true,
        writable: true,
        createdAt: "",
        updatedAt: "",
      },
    ]);
  });

  it("createPersona creates a global agent source", async () => {
    mockGooseSourcesCreate.mockResolvedValue({ source });

    const result = await createPersona({
      displayName: "Scout",
      avatar: { type: "url", value: source.properties.avatar },
      systemPrompt: "Research carefully.",
      provider: "goose",
      model: "claude-sonnet-4",
    });

    expect(mockGooseSourcesCreate).toHaveBeenCalledWith({
      type: "agent",
      name: "Scout",
      description: "Agent",
      content: "Research carefully.",
      properties: {
        provider: "goose",
        model: "claude-sonnet-4",
        avatar: source.properties.avatar,
      },
      global: true,
    });
    expect(result.displayName).toBe("Scout");
  });

  it("updatePersona loads existing agent source and updates by path", async () => {
    mockGooseSourcesList.mockResolvedValue({ sources: [source] });
    mockGooseSourcesUpdate.mockResolvedValue({
      source: { ...source, name: "Scout 2" },
    });

    const result = await updatePersona(source.path, {
      displayName: "Scout 2",
    });

    expect(mockGooseSourcesUpdate).toHaveBeenCalledWith({
      type: "agent",
      path: source.path,
      name: "Scout 2",
      description: "Agent",
      content: "Research carefully.",
      properties: {
        provider: "goose",
        model: "claude-sonnet-4",
        avatar: source.properties.avatar,
      },
    });
    expect(result.displayName).toBe("Scout 2");
  });

  it("deletePersona deletes an agent source by path", async () => {
    mockGooseSourcesDelete.mockResolvedValue(undefined);

    await deletePersona(source.path);

    expect(mockGooseSourcesDelete).toHaveBeenCalledWith({
      type: "agent",
      path: source.path,
    });
  });

  it("exportPersona exports an agent source", async () => {
    mockGooseSourcesExport.mockResolvedValue({
      json: '{"type":"agent"}',
      filename: "scout.agent.json",
    });

    const result = await exportPersona(source.path);

    expect(mockGooseSourcesExport).toHaveBeenCalledWith({
      type: "agent",
      path: source.path,
    });
    expect(result).toEqual({
      json: '{"type":"agent"}',
      suggestedFilename: "scout.agent.json",
    });
  });

  it("importPersonas imports agent source JSON", async () => {
    mockGooseSourcesImport.mockResolvedValue({ sources: [source] });
    const data = JSON.stringify({
      version: 1,
      type: "agent",
      name: "Scout",
      description: "Agent",
      content: "Research carefully.",
    });
    const fileBytes = Array.from(new TextEncoder().encode(data));

    const result = await importPersonas(fileBytes, "scout.agent.json");

    expect(mockGooseSourcesImport).toHaveBeenCalledWith({
      data,
      global: true,
    });
    expect(result).toHaveLength(1);
  });

  it("refreshPersonas lists personas", async () => {
    mockGooseSourcesList.mockResolvedValue({ sources: [source] });

    const result = await refreshPersonas();

    expect(mockGooseSourcesList).toHaveBeenCalledWith({ type: "agent" });
    expect(result).toHaveLength(1);
  });
});
