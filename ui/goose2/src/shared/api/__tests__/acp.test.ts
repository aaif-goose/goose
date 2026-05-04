import { beforeEach, describe, expect, it, vi } from "vitest";

const mockLoadSession = vi.fn();
const mockNewSession = vi.fn();
const mockSetProvider = vi.fn();
const mockSetModel = vi.fn();

vi.mock("../acpApi", () => ({
  listProviders: vi.fn(),
  prompt: vi.fn(),
  setModel: (...args: unknown[]) => mockSetModel(...args),
  setProvider: (...args: unknown[]) => mockSetProvider(...args),
  listSessions: vi.fn(),
  loadSession: (...args: unknown[]) => mockLoadSession(...args),
  newSession: (...args: unknown[]) => mockNewSession(...args),
  exportSession: vi.fn(),
  importSession: vi.fn(),
  forkSession: vi.fn(),
  cancelSession: vi.fn(),
}));

vi.mock("../acpNotificationHandler", () => ({
  setActiveMessageId: vi.fn(),
  clearActiveMessageId: vi.fn(),
}));

vi.mock("../sessionSearch", () => ({
  searchSessionsViaExports: vi.fn(),
}));

describe("acpLoadSession", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.resetModules();
  });

  it("restores the prior session mapping when replay loading fails", async () => {
    mockLoadSession.mockRejectedValueOnce(new Error("load failed"));

    const sessionTracker = await import("../acpSessionTracker");
    const { acpLoadSession } = await import("../acp");

    sessionTracker.registerSession(
      "local-session",
      "goose-session-1",
      "goose",
      "/tmp/original",
    );

    await expect(
      acpLoadSession("local-session", "goose-session-2", "/tmp/replay"),
    ).rejects.toThrow("load failed");

    expect(sessionTracker.getGooseSessionId("local-session")).toBe(
      "goose-session-1",
    );
    expect(sessionTracker.getLocalSessionId("goose-session-1")).toBe(
      "local-session",
    );
    expect(sessionTracker.getLocalSessionId("goose-session-2")).toBeNull();
  });
});

describe("acpCreateSession", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.resetModules();
  });

  it("uses the ACP session id as the UI session id", async () => {
    mockNewSession.mockResolvedValue({ sessionId: "acp-session-1" });

    const sessionTracker = await import("../acpSessionTracker");
    const { acpCreateSession } = await import("../acp");

    await expect(
      acpCreateSession("openai", "/tmp/project", {
        projectId: "project-1",
        personaId: "persona-1",
        modelId: "gpt-4.1",
      }),
    ).resolves.toEqual({ sessionId: "acp-session-1" });

    expect(mockNewSession).toHaveBeenCalledWith(
      "/tmp/project",
      "openai",
      "project-1",
      "persona-1",
    );
    expect(mockLoadSession).not.toHaveBeenCalled();
    expect(mockSetProvider).toHaveBeenCalledWith("acp-session-1", "openai");
    expect(mockSetModel).toHaveBeenCalledWith("acp-session-1", "gpt-4.1");
    expect(sessionTracker.getGooseSessionId("acp-session-1")).toBe(
      "acp-session-1",
    );
    expect(sessionTracker.getLocalSessionId("acp-session-1")).toBe(
      "acp-session-1",
    );
  });
});
