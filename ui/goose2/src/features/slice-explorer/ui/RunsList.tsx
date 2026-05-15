import { cn } from "@/shared/lib/cn";
import type { RunSummary } from "../types";

interface RunsListProps {
  runs: RunSummary[];
  selectedIds: Set<number>;
  onSelect: (runId: number) => void;
}

export function RunsList({ runs, selectedIds, onSelect }: RunsListProps) {
  return (
    <ul className="flex flex-col gap-1 overflow-y-auto p-2">
      {runs.map((run) => {
        const isSelected = selectedIds.has(run.id);
        return (
          <li key={run.id}>
            <button
              type="button"
              onClick={() => onSelect(run.id)}
              aria-pressed={isSelected}
              className={cn(
                "w-full rounded-md border px-3 py-2 text-left transition-colors",
                "hover:bg-background-muted",
                isSelected
                  ? "border-brand bg-background-muted"
                  : "border-border",
              )}
            >
              <div className="flex items-baseline justify-between gap-2">
                <span className="font-mono text-xs text-muted-foreground">
                  #{run.id}
                </span>
                <span className="font-mono text-xs">
                  pass<sup>{run.k}</sup>={formatPassPowK(run.passPowK)}
                </span>
              </div>
              <div className="mt-0.5 truncate text-sm" title={run.recipe}>
                {run.recipe}
              </div>
              <div className="mt-0.5 text-xs text-foreground-subtle">
                {shortenTime(run.startedAt)} · {run.nTrials} trials · k=
                {run.k}
              </div>
            </button>
          </li>
        );
      })}
    </ul>
  );
}

function formatPassPowK(value: number | null): string {
  return value == null ? "—" : value.toFixed(3);
}

function shortenTime(iso: string): string {
  // "2026-05-08T12:34:56.123456+00:00" → "2026-05-08 12:34"
  const noT = iso.replace("T", " ");
  const noFrac = noT.includes(".") ? noT.split(".", 2)[0] : noT;
  const noOffset = noFrac.includes("+")
    ? noFrac.slice(0, noFrac.lastIndexOf("+"))
    : noFrac;
  return noOffset.slice(0, 16);
}
