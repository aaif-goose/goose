import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { getRunDetail, listRuns } from "./sliceExplorer";

const invokeMock = vi.mocked(invoke);

describe("sliceExplorer api", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("forwards listRuns args to the Tauri command", async () => {
    invokeMock.mockResolvedValueOnce({
      runs: [],
      storePath: "/tmp/x.sqlite",
      storeMissing: false,
    });
    await listRuns({ limit: 5, recipe: "recipes/a" });
    expect(invokeMock).toHaveBeenCalledWith("eval_bench_list_runs", {
      limit: 5,
      recipe: "recipes/a",
      storePath: null,
    });
  });

  it("listRuns sends nulls for unspecified optional args", async () => {
    invokeMock.mockResolvedValueOnce({
      runs: [],
      storePath: "/tmp/x.sqlite",
      storeMissing: false,
    });
    await listRuns();
    expect(invokeMock).toHaveBeenCalledWith("eval_bench_list_runs", {
      limit: null,
      recipe: null,
      storePath: null,
    });
  });

  it("getRunDetail returns null when the backend has no such run", async () => {
    invokeMock.mockResolvedValueOnce(null);
    const detail = await getRunDetail(42);
    expect(detail).toBeNull();
    expect(invokeMock).toHaveBeenCalledWith("eval_bench_get_run", {
      runId: 42,
      storePath: null,
    });
  });
});
