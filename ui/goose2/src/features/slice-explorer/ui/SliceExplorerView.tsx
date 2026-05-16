import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { IconRefresh } from "@tabler/icons-react";
import { Button } from "@/shared/ui/button";
import { Spinner } from "@/shared/ui/spinner";
import { useRunDetail, useRunsList } from "../hooks/useEvalBenchData";
import { RunsList } from "./RunsList";
import { RunDetailPanel } from "./RunDetailPanel";
import { RunComparePanel } from "./RunComparePanel";

const RUNS_LIMIT = 50;

export function SliceExplorerView() {
  const { t } = useTranslation("sliceExplorer");
  const [compareMode, setCompareMode] = useState(false);
  const [selectedA, setSelectedA] = useState<number | null>(null);
  const [selectedB, setSelectedB] = useState<number | null>(null);

  const { data, loading, error, refresh } = useRunsList(RUNS_LIMIT);
  const detailA = useRunDetail(selectedA);
  const detailB = useRunDetail(compareMode ? selectedB : null);

  const selectedIds = useMemo(() => {
    const set = new Set<number>();
    if (selectedA != null) set.add(selectedA);
    if (compareMode && selectedB != null) set.add(selectedB);
    return set;
  }, [selectedA, selectedB, compareMode]);

  const handleSelect = useCallback(
    (runId: number) => {
      if (!compareMode) {
        setSelectedA(runId);
        return;
      }
      if (selectedA == null) {
        setSelectedA(runId);
      } else if (runId === selectedA) {
        setSelectedA(null);
      } else if (selectedB === runId) {
        setSelectedB(null);
      } else {
        setSelectedB(runId);
      }
    },
    [compareMode, selectedA, selectedB],
  );

  const handleToggleCompare = useCallback(() => {
    setCompareMode((prev) => {
      if (prev) {
        setSelectedB(null);
      }
      return !prev;
    });
  }, []);

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center justify-between border-b border-border px-4 py-3">
        <div>
          <h1 className="text-base font-semibold">
            {t("title", "Slice Explorer")}
          </h1>
          <p className="text-xs text-foreground-subtle">
            {data?.storePath ?? ""}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant={compareMode ? "default" : "outline"}
            size="sm"
            onClick={handleToggleCompare}
            type="button"
            aria-pressed={compareMode}
          >
            {t("compare.toggle", "Compare two runs")}
          </Button>
          <Button
            variant="outline"
            size="icon"
            onClick={() => void refresh()}
            disabled={loading}
            aria-label={t("actions.refresh", "Refresh runs")}
            type="button"
          >
            <IconRefresh size={16} />
          </Button>
        </div>
      </header>

      <div className="flex flex-1 min-h-0 overflow-hidden">
        <aside className="w-80 flex-shrink-0 overflow-y-auto border-r border-border">
          {loading && !data ? (
            <div className="flex h-full items-center justify-center">
              <Spinner className="size-5 text-brand" />
            </div>
          ) : error ? (
            <div className="p-4 text-sm text-danger">{error}</div>
          ) : data?.storeMissing ? (
            <div className="p-4 text-sm text-muted-foreground">
              {t(
                "empty.noStore",
                "No eval-bench store yet. Run a recipe with eval-bench/run_kpass.py to populate ~/.skein/eval-bench.sqlite.",
              )}
            </div>
          ) : !data || data.runs.length === 0 ? (
            <div className="p-4 text-sm text-muted-foreground">
              {t("empty.noRuns", "No runs recorded yet.")}
            </div>
          ) : (
            <RunsList
              runs={data.runs}
              selectedIds={selectedIds}
              onSelect={handleSelect}
            />
          )}
        </aside>

        <main className="flex-1 overflow-y-auto">
          <DetailArea
            compareMode={compareMode}
            detailA={detailA}
            detailB={detailB}
          />
        </main>
      </div>
    </div>
  );
}

interface DetailAreaProps {
  compareMode: boolean;
  detailA: ReturnType<typeof useRunDetail>;
  detailB: ReturnType<typeof useRunDetail>;
}

function DetailArea({ compareMode, detailA, detailB }: DetailAreaProps) {
  const { t } = useTranslation("sliceExplorer");

  if (compareMode) {
    if (!detailA.detail || !detailB.detail) {
      return (
        <Placeholder
          message={t(
            "compare.prompt",
            "Pick two runs from the list to compare.",
          )}
        />
      );
    }
    return <RunComparePanel a={detailA.detail} b={detailB.detail} />;
  }

  if (detailA.loading) {
    return (
      <div className="flex h-full items-center justify-center">
        <Spinner className="size-5 text-brand" />
      </div>
    );
  }
  if (detailA.error) {
    return <Placeholder message={detailA.error} tone="danger" />;
  }
  if (!detailA.detail) {
    return (
      <Placeholder
        message={t(
          "detail.prompt",
          "Select a run from the list to see its slice breakdown.",
        )}
      />
    );
  }
  return <RunDetailPanel detail={detailA.detail} />;
}

function Placeholder({ message, tone }: { message: string; tone?: "danger" }) {
  return (
    <div className="flex h-full items-center justify-center p-8 text-center">
      <p
        className={
          tone === "danger"
            ? "text-sm text-danger"
            : "text-sm text-muted-foreground"
        }
      >
        {message}
      </p>
    </div>
  );
}
