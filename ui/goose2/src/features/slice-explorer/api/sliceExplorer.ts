import { invoke } from "@tauri-apps/api/core";
import type { ListRunsResult, RunDetail } from "../types";

export interface ListRunsArgs {
  limit?: number;
  recipe?: string;
  storePath?: string;
}

export async function listRuns(
  args: ListRunsArgs = {},
): Promise<ListRunsResult> {
  return await invoke<ListRunsResult>("eval_bench_list_runs", {
    limit: args.limit ?? null,
    recipe: args.recipe ?? null,
    storePath: args.storePath ?? null,
  });
}

export async function getRunDetail(
  runId: number,
  storePath?: string,
): Promise<RunDetail | null> {
  return await invoke<RunDetail | null>("eval_bench_get_run", {
    runId,
    storePath: storePath ?? null,
  });
}
