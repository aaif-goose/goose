import { useMemo } from "react";
import { computePassk, passkByAxis, recordedAxes } from "../lib/passk";
import type { RunDetail } from "../types";

interface RunDetailPanelProps {
  detail: RunDetail;
}

export function RunDetailPanel({ detail }: RunDetailPanelProps) {
  const { run, trials } = detail;

  const overall = useMemo(() => {
    if (trials.length === 0) return null;
    return computePassk(
      trials.map((t) => t.passed),
      Math.min(run.k, trials.length),
    );
  }, [trials, run.k]);

  const axes = useMemo(() => recordedAxes(trials), [trials]);

  return (
    <section className="flex flex-col gap-6 p-6">
      <header className="flex flex-col gap-1">
        <h2 className="text-lg font-semibold">
          Run #{run.id}{" "}
          <span className="text-foreground-subtle font-normal">
            · {run.recipe}
          </span>
        </h2>
        <dl className="grid grid-cols-2 gap-x-6 gap-y-1 text-sm text-muted-foreground">
          <div className="flex gap-2">
            <dt className="font-mono text-xs">k:</dt>
            <dd>{run.k}</dd>
          </div>
          <div className="flex gap-2">
            <dt className="font-mono text-xs">trials:</dt>
            <dd>{run.nTrials}</dd>
          </div>
          <div className="flex gap-2">
            <dt className="font-mono text-xs">started:</dt>
            <dd>{run.startedAt}</dd>
          </div>
          <div className="flex gap-2">
            <dt className="font-mono text-xs">finished:</dt>
            <dd>{run.finishedAt ?? "(in progress)"}</dd>
          </div>
          {run.notes && (
            <div className="col-span-2 flex gap-2">
              <dt className="font-mono text-xs">notes:</dt>
              <dd>{run.notes}</dd>
            </div>
          )}
        </dl>
      </header>

      {trials.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          No trials recorded yet for this run.
        </p>
      ) : (
        <>
          <div className="rounded-md border border-border bg-background-muted p-4">
            <h3 className="mb-2 text-sm font-semibold">Overall</h3>
            <div className="grid grid-cols-2 gap-3 font-mono text-sm">
              <span>
                pass@{run.k} ={" "}
                <strong>{overall ? overall.passAtK.toFixed(3) : "—"}</strong>
              </span>
              <span>
                pass<sup>{run.k}</sup> ={" "}
                <strong>{overall ? overall.passPowK.toFixed(3) : "—"}</strong>
              </span>
            </div>
          </div>

          {axes.length === 0 ? (
            <p className="text-sm text-muted-foreground">
              No axes recorded on these trials.
            </p>
          ) : (
            axes.map((axis) => (
              <AxisBreakdown key={axis} axis={axis} trials={trials} k={run.k} />
            ))
          )}
        </>
      )}
    </section>
  );
}

interface AxisBreakdownProps {
  axis: string;
  trials: RunDetail["trials"];
  k: number;
}

function AxisBreakdown({ axis, trials, k }: AxisBreakdownProps) {
  const cells = useMemo(() => passkByAxis(trials, axis, k), [trials, axis, k]);
  if (cells.length === 0) return null;
  return (
    <div className="rounded-md border border-border">
      <h3 className="border-b border-border px-4 py-2 text-sm font-semibold">
        Slice by <span className="font-mono">{axis}</span>
      </h3>
      <table className="w-full text-sm">
        <thead className="text-xs text-foreground-subtle">
          <tr>
            <th className="px-4 py-1 text-left font-normal">value</th>
            <th className="px-4 py-1 text-right font-normal">n</th>
            <th className="px-4 py-1 text-right font-normal">pass@k</th>
            <th className="px-4 py-1 text-right font-normal">
              pass<sup>k</sup>
            </th>
          </tr>
        </thead>
        <tbody>
          {cells.map((cell) => (
            <tr key={cell.value} className="border-t border-border">
              <td className="px-4 py-1 font-mono">{cell.value}</td>
              <td className="px-4 py-1 text-right font-mono">{cell.nTrials}</td>
              <td className="px-4 py-1 text-right font-mono">
                {cell.passAtK.toFixed(3)}
              </td>
              <td className="px-4 py-1 text-right font-mono">
                {cell.passPowK.toFixed(3)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
