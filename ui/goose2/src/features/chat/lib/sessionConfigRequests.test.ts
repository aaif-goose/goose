import { beforeEach, describe, expect, it, vi } from "vitest";
import { applyLatestSessionConfig } from "./sessionConfigRequests";

const mockAcpPrepareSession = vi.fn();
const mockAcpSetModel = vi.fn();

vi.mock("@/shared/api/acp", () => ({
  acpPrepareSession: (...args: unknown[]) => mockAcpPrepareSession(...args),
  acpSetModel: (...args: unknown[]) => mockAcpSetModel(...args),
}));

function deferred<T = void>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((promiseResolve, promiseReject) => {
    resolve = promiseResolve;
    reject = promiseReject;
  });
  return { promise, resolve, reject };
}

describe("applyLatestSessionConfig", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockAcpPrepareSession.mockResolvedValue(undefined);
    mockAcpSetModel.mockResolvedValue(undefined);
  });

  it("replays the latest provider and model after a stale request finishes", async () => {
    const oldPrepare = deferred();
    const oldSetModel = deferred();
    const newPrepare = deferred();
    const newSetModel = deferred();

    mockAcpPrepareSession.mockImplementation(
      (_sessionId: string, providerId: string) =>
        providerId === "old-provider" ? oldPrepare.promise : newPrepare.promise,
    );
    mockAcpSetModel.mockImplementation((_sessionId: string, modelId: string) =>
      modelId === "old-model" ? oldSetModel.promise : newSetModel.promise,
    );

    const oldResult = applyLatestSessionConfig({
      sessionId: "session-latest",
      providerId: "old-provider",
      workingDir: "/old",
      modelId: "old-model",
    });
    const newResult = applyLatestSessionConfig({
      sessionId: "session-latest",
      providerId: "new-provider",
      workingDir: "/new",
      modelId: "new-model",
    });

    await vi.waitFor(() => {
      expect(mockAcpPrepareSession).toHaveBeenCalledWith(
        "session-latest",
        "old-provider",
        "/old",
      );
    });

    oldPrepare.resolve();
    await vi.waitFor(() => {
      expect(mockAcpSetModel).toHaveBeenCalledWith(
        "session-latest",
        "old-model",
      );
    });

    oldSetModel.resolve();
    await vi.waitFor(() => {
      expect(mockAcpPrepareSession).toHaveBeenCalledWith(
        "session-latest",
        "new-provider",
        "/new",
      );
    });

    newPrepare.resolve();
    await vi.waitFor(() => {
      expect(mockAcpSetModel).toHaveBeenCalledWith(
        "session-latest",
        "new-model",
      );
    });

    newSetModel.resolve();

    await expect(oldResult).resolves.toEqual({ applied: false });
    await expect(newResult).resolves.toEqual({ applied: true });
  });

  it("continues to the latest request when a stale request fails", async () => {
    const oldPrepare = deferred();
    const newPrepare = deferred();
    const newSetModel = deferred();

    mockAcpPrepareSession.mockImplementation(
      (_sessionId: string, providerId: string) =>
        providerId === "old-provider" ? oldPrepare.promise : newPrepare.promise,
    );
    mockAcpSetModel.mockReturnValue(newSetModel.promise);

    const oldResult = applyLatestSessionConfig({
      sessionId: "session-stale-failure",
      providerId: "old-provider",
      workingDir: "/old",
      modelId: "old-model",
    });
    const newResult = applyLatestSessionConfig({
      sessionId: "session-stale-failure",
      providerId: "new-provider",
      workingDir: "/new",
      modelId: "new-model",
    });

    oldPrepare.reject(new Error("old prepare failed"));
    await vi.waitFor(() => {
      expect(mockAcpPrepareSession).toHaveBeenCalledWith(
        "session-stale-failure",
        "new-provider",
        "/new",
      );
    });

    newPrepare.resolve();
    await vi.waitFor(() => {
      expect(mockAcpSetModel).toHaveBeenCalledWith(
        "session-stale-failure",
        "new-model",
      );
    });
    newSetModel.resolve();

    await expect(oldResult).resolves.toEqual({ applied: false });
    await expect(newResult).resolves.toEqual({ applied: true });
  });

  it("treats superseded requests as applied when the full session config matches", async () => {
    const firstPrepare = deferred();
    const firstSetModel = deferred();

    // First call gets deferred promises; subsequent calls resolve immediately
    mockAcpPrepareSession.mockReturnValueOnce(firstPrepare.promise);
    mockAcpSetModel.mockReturnValueOnce(firstSetModel.promise);

    // Both requests have identical config (provider, workingDir, model)
    const oldResult = applyLatestSessionConfig({
      sessionId: "session-same-config",
      providerId: "openai",
      workingDir: "/project",
      modelId: "gpt-5.4",
    });
    const newResult = applyLatestSessionConfig({
      sessionId: "session-same-config",
      providerId: "openai",
      workingDir: "/project",
      modelId: "gpt-5.4",
    });

    // The queue executes the first request's prepare (stale)
    await vi.waitFor(() => {
      expect(mockAcpPrepareSession).toHaveBeenCalledWith(
        "session-same-config",
        "openai",
        "/project",
      );
    });
    firstPrepare.resolve();

    // Then the first request's setModel
    await vi.waitFor(() => {
      expect(mockAcpSetModel).toHaveBeenCalledWith(
        "session-same-config",
        "gpt-5.4",
      );
    });
    firstSetModel.resolve();

    // After the stale request finishes, the queue replays the latest
    // (which has the same config and resolves immediately via default mock)
    // Both resolve as applied since the final config matches both requests
    await expect(oldResult).resolves.toEqual({ applied: true });
    await expect(newResult).resolves.toEqual({ applied: true });
  });

  it("treats superseded requests as not applied when workingDir differs", async () => {
    const firstPrepare = deferred();
    const firstSetModel = deferred();

    // First call gets deferred promises; subsequent calls resolve immediately
    mockAcpPrepareSession.mockReturnValueOnce(firstPrepare.promise);
    mockAcpSetModel.mockReturnValueOnce(firstSetModel.promise);

    const oldResult = applyLatestSessionConfig({
      sessionId: "session-diff-dir",
      providerId: "openai",
      workingDir: "/old",
      modelId: "gpt-5.4",
    });
    const newResult = applyLatestSessionConfig({
      sessionId: "session-diff-dir",
      providerId: "openai",
      workingDir: "/new",
      modelId: "gpt-5.4",
    });

    // First request executes (stale) with workingDir "/old"
    await vi.waitFor(() => {
      expect(mockAcpPrepareSession).toHaveBeenCalledWith(
        "session-diff-dir",
        "openai",
        "/old",
      );
    });
    firstPrepare.resolve();

    await vi.waitFor(() => {
      expect(mockAcpSetModel).toHaveBeenCalledWith(
        "session-diff-dir",
        "gpt-5.4",
      );
    });
    firstSetModel.resolve();

    // After the stale request finishes, the queue replays the latest
    // with workingDir "/new" (resolves immediately via default mock)
    // Old request is not applied (workingDir differs), new one is applied
    await expect(oldResult).resolves.toEqual({ applied: false });
    await expect(newResult).resolves.toEqual({ applied: true });
  });
});
