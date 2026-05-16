import { useCallback, useEffect, useState } from "react";
import { getRunDetail, listRuns } from "../api/sliceExplorer";
import type { ListRunsResult, RunDetail } from "../types";

export interface RunsState {
  data: ListRunsResult | null;
  loading: boolean;
  error: string | null;
}

export function useRunsList(limit: number) {
  const [state, setState] = useState<RunsState>({
    data: null,
    loading: true,
    error: null,
  });

  const refresh = useCallback(async () => {
    setState((prev) => ({ ...prev, loading: true, error: null }));
    try {
      const data = await listRuns({ limit });
      setState({ data, loading: false, error: null });
    } catch (err) {
      setState({
        data: null,
        loading: false,
        error: err instanceof Error ? err.message : String(err),
      });
    }
  }, [limit]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return { ...state, refresh };
}

export interface RunDetailState {
  detail: RunDetail | null;
  loading: boolean;
  error: string | null;
}

export function useRunDetail(runId: number | null) {
  const [state, setState] = useState<RunDetailState>({
    detail: null,
    loading: false,
    error: null,
  });

  useEffect(() => {
    if (runId == null) {
      setState({ detail: null, loading: false, error: null });
      return;
    }
    let cancelled = false;
    setState({ detail: null, loading: true, error: null });
    getRunDetail(runId)
      .then((detail) => {
        if (!cancelled) {
          setState({ detail, loading: false, error: null });
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          setState({
            detail: null,
            loading: false,
            error: err instanceof Error ? err.message : String(err),
          });
        }
      });
    return () => {
      cancelled = true;
    };
  }, [runId]);

  return state;
}
