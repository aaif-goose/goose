import {
  IconArrowRight,
  IconCheck,
  IconDatabaseImport,
} from "@tabler/icons-react";
import { formatCandidateCounts } from "../lib/importCounts";
import type { OnboardingImportCandidate, TFunctionLike } from "../types";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { Spinner } from "@/shared/ui/spinner";

interface ImportStepProps {
  candidates: OnboardingImportCandidate[];
  selectedCandidateIds: Set<string>;
  scanLoading: boolean;
  importing: boolean;
  importError: string;
  onToggleCandidate: (id: string) => void;
  onSkip: () => void;
  onContinue: () => void;
  t: TFunctionLike;
}

export function ImportStep({
  candidates,
  selectedCandidateIds,
  scanLoading,
  importing,
  importError,
  onToggleCandidate,
  onSkip,
  onContinue,
  t,
}: ImportStepProps) {
  const hasCandidates = candidates.length > 0;

  return (
    <section className="flex w-full flex-col items-center">
      <div className="flex size-14 items-center justify-center rounded-[14px] bg-muted text-muted-foreground">
        <IconDatabaseImport className="size-6" strokeWidth={1.75} />
      </div>
      <h2 className="mt-6 text-[22px] font-semibold tracking-tight text-foreground">
        {t("import.title")}
      </h2>
      <p className="mt-2 max-w-[420px] text-sm leading-6 text-muted-foreground">
        {t("import.description")}
      </p>

      <div className="mt-8 w-full space-y-2">
        {scanLoading ? (
          <div className="flex items-center justify-center gap-2 rounded-[14px] border border-border px-4 py-5 text-sm text-muted-foreground">
            <Spinner className="size-4 text-foreground" />
            {t("import.scanning")}
          </div>
        ) : !hasCandidates ? (
          <div className="rounded-[14px] border border-border px-4 py-5 text-left">
            <p className="text-sm font-medium">{t("import.emptyTitle")}</p>
            <p className="mt-1 text-sm text-muted-foreground">
              {t("import.emptyDescription")}
            </p>
          </div>
        ) : (
          candidates.map((candidate) => {
            const warnings = candidate.warnings ?? [];
            const checked = selectedCandidateIds.has(candidate.id);
            return (
              <label
                key={candidate.id}
                className={cn(
                  "flex cursor-pointer items-center gap-3 rounded-[14px] border bg-background px-4 py-3 text-left transition-colors",
                  checked
                    ? "border-foreground/80"
                    : "border-border hover:border-foreground/30",
                )}
              >
                <input
                  type="checkbox"
                  className="sr-only"
                  checked={checked}
                  onChange={() => onToggleCandidate(candidate.id)}
                />
                <span className="flex size-10 shrink-0 items-center justify-center rounded-[10px] bg-muted">
                  <IconDatabaseImport
                    className="size-5 text-foreground"
                    strokeWidth={1.75}
                  />
                </span>
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className="truncate text-sm font-medium text-foreground">
                    {candidate.displayName}
                  </span>
                  <span className="mt-0.5 truncate text-xs text-muted-foreground">
                    {formatCandidateCounts(candidate, t)}
                  </span>
                  {warnings.length > 0 ? (
                    <span className="mt-1 block text-xs text-text-warning">
                      {warnings.join(" ")}
                    </span>
                  ) : null}
                </span>
                <span
                  aria-hidden="true"
                  className={cn(
                    "flex size-5 shrink-0 items-center justify-center rounded-full transition-colors",
                    checked
                      ? "bg-foreground text-background"
                      : "border border-border",
                  )}
                >
                  {checked ? (
                    <IconCheck className="size-3" strokeWidth={3} />
                  ) : null}
                </span>
              </label>
            );
          })
        )}
      </div>

      {importError ? (
        <p
          role="alert"
          className="mt-4 w-full rounded-[10px] bg-danger/10 px-3 py-2 text-xs text-danger"
        >
          {importError}
        </p>
      ) : null}

      <div className="mt-6 flex items-center justify-center gap-3">
        <Button type="button" variant="ghost" onClick={onSkip}>
          {t("import.skip")}
        </Button>
        <Button
          type="button"
          onClick={onContinue}
          disabled={importing || scanLoading}
          rightIcon={!importing ? <IconArrowRight /> : undefined}
        >
          {importing ? <Spinner className="size-3.5 text-current" /> : null}
          {!hasCandidates ? t("import.next") : t("import.continue")}
        </Button>
      </div>
    </section>
  );
}
