export interface RunSummary {
  id: number;
  recipe: string;
  startedAt: string;
  finishedAt: string | null;
  k: number;
  notes: string | null;
  nTrials: number;
  passPowK: number | null;
}

export interface Trial {
  taskId: string;
  trialIndex: number;
  passed: boolean;
  polarity: string;
  tags: string[];
  axes: Record<string, string | number | boolean | null>;
  durationMs: number | null;
}

export interface RunDetail {
  run: RunSummary;
  trials: Trial[];
}

export interface ListRunsResult {
  runs: RunSummary[];
  storePath: string;
  storeMissing: boolean;
}
