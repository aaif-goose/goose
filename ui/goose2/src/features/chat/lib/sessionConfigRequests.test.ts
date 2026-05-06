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

  it("treats superseded requests as applied when the latest provider and model match", async () => {
    const oldPrepare = deferred();
    const oldSetModel = deferred();
    const newPrepare = deferred();
    const newSetModel = deferred();

    mockAcpPrepareSession.mockImplementation(
      (_sessionId: string, _providerId: string, workingDir: string) =>
        workingDir === "/old" ? oldPrepare.promise : newPrepare.promise,
    );
    mockAcpSetModel
      .mockReturnValueOnce(oldSetModel.promise)
      .mockReturnValueOnce(newSetModel.promise);

    const oldResult = applyLatestSessionConfig({
      sessionId: "session-same-model",
      providerId: "openai",
      workingDir: "/old",
      modelId: "gpt-5.4",
    });
    const newResult = applyLatestSessionConfig({
      sessionId: "session-same-model",
      providerId: "openai",
      workingDir: "/new",
      modelId: "gpt-5.4",
    });

    oldPrepare.resolve();
    await vi.waitFor(() => {
      expect(mockAcpSetModel).toHaveBeenCalledWith(
        "session-same-model",
        "gpt-5.4",
      );
    });
    oldSetModel.resolve();
    await vi.waitFor(() => {
      expect(mockAcpPrepareSession).toHaveBeenCalledWith(
        "session-same-model",
        "openai",
        "/new",
      );
    });
    newPrepare.resolve();
    await vi.waitFor(() => {
      expect(mockAcpSetModel).toHaveBeenCalledTimes(2);
    });
    newSetModel.resolve();

    await expect(oldResult).resolves.toEqual({ applied: true });
    await expect(newResult).resolves.toEqual({ applied: true });
  });
});
