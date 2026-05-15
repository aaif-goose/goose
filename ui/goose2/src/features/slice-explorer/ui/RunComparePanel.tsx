import { useMemo } from "react";
import { cn } from "@/shared/lib/cn";
import { computePassk, passkByAxis, recordedAxes } from "../lib/passk";
import type { RunDetail } from "../types";

interface RunComparePanelProps {
  a: RunDetail;
  b: RunDetail;
}

interface OverallStats {
  passAtK: number;
  passPowK: number;
}

function computeOverall(detail: RunDetail): OverallStats | null {
  if (detail.trials.length === 0) return null;
  return computePassk(
    detail.trials.map((t) => t.passed),
    Math.min(detail.run.k, detail.trials.length),
  );
}

export function RunComparePanel({ a, b }: RunComparePanelProps) {
  const overallA = useMemo(() => computeOverall(a), [a]);
  const overallB = useMemo(() => computeOverall(b), [b]);

  const sharedAxes = useMemo(() => {
    const axesA = new Set(recordedAxes(a.trials));
    const axesB = new Set(recordedAxes(b.trials));
    return Array.from(axesA)
      .filter((axis) => axesB.has(axis))
      .sort();
  }, [a.trials, b.trials]);

  return (
    <section className="flex flex-col gap-6 p-6">
      <header className="flex flex-col gap-1">
        <h2 className="text-lg font-semibold">
          Compare #{a.run.id} → #{b.run.id}
        </h2>
        <p className="text-sm text-muted-foreground">
          <span className="font-mono">a:</span> {a.run.recipe} ·{" "}
          <span className="font-mono">b:</span> {b.run.recipe}
        </p>
        {a.run.recipe !== b.run.recipe && (
          <p className="text-xs text-text-warning">
            Comparing runs from different recipes.
          </p>
        )}
        {a.run.k !== b.run.k && (
          <p className="text-xs text-text-warning">
            Different k: a.k={a.run.k}, b.k={b.run.k}.
          </p>
        )}
      </header>

      <div className="rounded-md border border-border bg-background-muted p-4">
        <h3 className="mb-2 text-sm font-semibold">Overall</h3>
        <div className="grid grid-cols-3 gap-3 font-mono text-sm">
          <span>
            pass<sup>{a.run.k}</sup> a ={" "}
            <strong>{overallA ? overallA.passPowK.toFixed(3) : "—"}</strong>
          </span>
          <span>
            pass<sup>{b.run.k}</sup> b ={" "}
            <strong>{overallB ? overallB.passPowK.toFixed(3) : "—"}</strong>
          </span>
          {overallA && overallB && (
            <DeltaCell delta={overallB.passPowK - overallA.passPowK} />
          )}
        </div>
      </div>

      {sharedAxes.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          No shared axes between these runs.
        </p>
      ) : (
        sharedAxes.map((axis) => (
          <CompareAxisTable key={axis} axis={axis} a={a} b={b} />
        ))
      )}
    </section>
  );
}

interface CompareAxisTableProps {
  axis: string;
  a: RunDetail;
  b: RunDetail;
}

function CompareAxisTable({ axis, a, b }: CompareAxisTableProps) {
  const cellsA = useMemo(() => passkByAxis(a.trials, axis, a.run.k), [a, axis]);
  const cellsB = useMemo(() => passkByAxis(b.trials, axis, b.run.k), [b, axis]);

  const mapA = new Map(cellsA.map((c) => [c.value, c.passPowK]));
  const mapB = new Map(cellsB.map((c) => [c.value, c.passPowK]));
  const values = Array.from(new Set([...mapA.keys(), ...mapB.keys()])).sort();

  return (
    <div className="rounded-md border border-border">
      <h3 className="border-b border-border px-4 py-2 text-sm font-semibold">
        Slice by <span className="font-mono">{axis}</span>
      </h3>
      <table className="w-full text-sm">
        <thead className="text-xs text-foreground-subtle">
          <tr>
            <th className="px-4 py-1 text-left font-normal">value</th>
            <th className="px-4 py-1 text-right font-normal">
              pass<sup>k</sup> a
            </th>
            <th className="px-4 py-1 text-right font-normal">
              pass<sup>k</sup> b
            </th>
            <th className="px-4 py-1 text-right font-normal">Δ</th>
          </tr>
        </thead>
        <tbody>
          {values.map((value) => {
            const valA = mapA.get(value);
            const valB = mapB.get(value);
            const delta = valA != null && valB != null ? valB - valA : null;
            return (
              <tr key={value} className="border-t border-border">
                <td className="px-4 py-1 font-mono">{value}</td>
                <td className="px-4 py-1 text-right font-mono">
                  {valA != null ? valA.toFixed(3) : "—"}
                </td>
                <td className="px-4 py-1 text-right font-mono">
                  {valB != null ? valB.toFixed(3) : "—"}
                </td>
                <td className="px-4 py-1 text-right font-mono">
                  {delta != null ? <DeltaCell delta={delta} /> : "—"}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function DeltaCell({ delta }: { delta: number }) {
  const arrow = delta > 0.005 ? "↑" : delta < -0.005 ? "↓" : "·";
  const color = cn(
    "font-mono",
    delta > 0.005 && "text-text-success",
    delta < -0.005 && "text-danger",
  );
  return (
    <span className={color}>
      {delta >= 0 ? "+" : ""}
      {delta.toFixed(3)} {arrow}
    </span>
  );
}
