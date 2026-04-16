import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/button";
import {
  cancelDictationLocalModelDownload,
  deleteDictationLocalModel,
  downloadDictationLocalModel,
  getDictationLocalModelDownloadProgress,
  listDictationLocalModels,
} from "@/shared/api/dictation";

type LocalModel = {
  id: string;
  description: string;
  sizeMb: number;
  downloaded: boolean;
  downloadInProgress: boolean;
};

type DownloadProgress = {
  bytesDownloaded: number;
  totalBytes: number;
  progressPercent: number;
  status: string;
  error?: string | null;
};

const POLL_INTERVAL_MS = 750;

interface LocalWhisperModelsProps {
  selectedModelId: string;
  onSelectModel: (modelId: string) => void | Promise<void>;
  onModelsChanged: () => void | Promise<void>;
}

export function LocalWhisperModels({
  selectedModelId,
  onSelectModel,
  onModelsChanged,
}: LocalWhisperModelsProps) {
  const { t } = useTranslation(["settings", "common"]);
  const [models, setModels] = useState<LocalModel[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [downloadingIds, setDownloadingIds] = useState<Set<string>>(new Set());
  const [progresses, setProgresses] = useState<Map<string, DownloadProgress>>(
    new Map(),
  );
  const onModelsChangedRef = useRef(onModelsChanged);
  onModelsChangedRef.current = onModelsChanged;

  const refresh = useCallback(async () => {
    try {
      const list =
        (await listDictationLocalModels()) as unknown as LocalModel[];
      setModels(list);
      setDownloadingIds((prev) => {
        const next = new Set(prev);
        for (const m of list) {
          if (m.downloadInProgress) next.add(m.id);
        }
        return next;
      });
    } catch (err) {
      setError(
        err instanceof Error ? err.message : t("general.voiceInput.loadError"),
      );
    }
  }, [t]);

  useEffect(() => {
    const load = async () => {
      setLoading(true);
      setError(null);
      await refresh();
      setLoading(false);
    };
    void load();
  }, [refresh]);

  useEffect(() => {
    if (downloadingIds.size === 0) return;
    let cancelled = false;

    const tick = async () => {
      const next = new Map<string, DownloadProgress>();
      const stillActive = new Set<string>();
      const finishedIds: string[] = [];

      for (const id of downloadingIds) {
        try {
          const progress = (await getDictationLocalModelDownloadProgress(
            id,
          )) as unknown as DownloadProgress | null;
          if (!progress) {
            finishedIds.push(id);
            continue;
          }
          next.set(id, progress);
          if (progress.status === "downloading") {
            stillActive.add(id);
          } else {
            finishedIds.push(id);
          }
        } catch {
          stillActive.add(id);
        }
      }
      if (cancelled) return;
      setProgresses(next);
      if (finishedIds.length > 0) {
        await refresh();
        await onModelsChangedRef.current();
      }
      setDownloadingIds(stillActive);
    };

    const interval = window.setInterval(() => {
      void tick();
    }, POLL_INTERVAL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, [downloadingIds, refresh]);

  const startDownload = useCallback(
    async (modelId: string) => {
      setError(null);
      try {
        await downloadDictationLocalModel(modelId);
        setDownloadingIds((prev) => new Set(prev).add(modelId));
      } catch (err) {
        setError(
          err instanceof Error
            ? err.message
            : t("general.voiceInput.saveError"),
        );
      }
    },
    [t],
  );

  const cancelDownload = useCallback(
    async (modelId: string) => {
      setError(null);
      try {
        await cancelDictationLocalModelDownload(modelId);
      } catch (err) {
        setError(
          err instanceof Error
            ? err.message
            : t("general.voiceInput.saveError"),
        );
      } finally {
        setDownloadingIds((prev) => {
          const next = new Set(prev);
          next.delete(modelId);
          return next;
        });
        await refresh();
      }
    },
    [refresh, t],
  );

  const deleteModel = useCallback(
    async (modelId: string) => {
      setError(null);
      try {
        await deleteDictationLocalModel(modelId);
        await refresh();
        await onModelsChanged();
      } catch (err) {
        setError(
          err instanceof Error
            ? err.message
            : t("general.voiceInput.deleteError"),
        );
      }
    },
    [onModelsChanged, refresh, t],
  );

  if (loading) {
    return (
      <div className="rounded-lg border border-border px-3 py-3">
        <p className="text-xs text-muted-foreground">
          {t("common:labels.loading")}
        </p>
      </div>
    );
  }

  if (models.length === 0) {
    return (
      <div className="rounded-lg border border-border px-3 py-3">
        <p className="text-xs text-muted-foreground">
          {t("general.voiceInput.noLocalModels")}
        </p>
      </div>
    );
  }

  return (
    <div className="space-y-2 rounded-lg border border-border px-3 py-3">
      <div>
        <p className="text-xs font-medium text-foreground">
          {t("general.voiceInput.localModelLabel")}
        </p>
        <p className="mt-1 text-xs text-muted-foreground">
          {t("general.voiceInput.localModelDescription")}
        </p>
      </div>

      <ul className="divide-y divide-border">
        {models.map((model) => {
          const progress = progresses.get(model.id);
          const isDownloading =
            downloadingIds.has(model.id) ||
            progress?.status === "downloading" ||
            model.downloadInProgress;
          const isSelected = model.downloaded && model.id === selectedModelId;
          return (
            <li
              key={model.id}
              className="flex items-start justify-between gap-3 py-2"
            >
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <p className="truncate text-xs font-medium text-foreground">
                    {model.id}
                  </p>
                  <span className="text-xs text-muted-foreground">
                    {model.sizeMb} MB
                  </span>
                  {isSelected ? (
                    <span className="rounded-full bg-primary/10 px-2 py-0.5 text-[10px] font-medium text-primary">
                      {t("general.voiceInput.selectedModel")}
                    </span>
                  ) : null}
                </div>
                <p className="mt-1 text-xs text-muted-foreground">
                  {model.description}
                </p>
                {isDownloading && progress ? (
                  <div className="mt-2 space-y-1">
                    <div className="h-1.5 w-full overflow-hidden rounded-full bg-muted">
                      <div
                        className="h-full bg-primary transition-all"
                        style={{
                          width: `${Math.max(0, Math.min(100, progress.progressPercent))}%`,
                        }}
                      />
                    </div>
                    <p className="text-[10px] text-muted-foreground">
                      {t("general.voiceInput.downloadProgress", {
                        percent: Math.round(progress.progressPercent),
                      })}
                    </p>
                  </div>
                ) : null}
                {progress?.status === "failed" && progress.error ? (
                  <p className="mt-1 text-xs text-destructive">
                    {progress.error}
                  </p>
                ) : null}
              </div>

              <div className="flex flex-shrink-0 gap-2">
                {isDownloading ? (
                  <Button
                    type="button"
                    size="sm"
                    variant="outline-flat"
                    onClick={() => void cancelDownload(model.id)}
                  >
                    {t("common:actions.cancel")}
                  </Button>
                ) : model.downloaded ? (
                  <>
                    {!isSelected ? (
                      <Button
                        type="button"
                        size="sm"
                        variant="outline-flat"
                        onClick={() => void onSelectModel(model.id)}
                      >
                        {t("general.voiceInput.selectModel")}
                      </Button>
                    ) : null}
                    <Button
                      type="button"
                      size="sm"
                      variant="ghost"
                      className="text-destructive hover:text-destructive"
                      onClick={() => void deleteModel(model.id)}
                    >
                      {t("general.voiceInput.deleteModel")}
                    </Button>
                  </>
                ) : (
                  <Button
                    type="button"
                    size="sm"
                    onClick={() => void startDownload(model.id)}
                  >
                    {t("general.voiceInput.download")}
                  </Button>
                )}
              </div>
            </li>
          );
        })}
      </ul>

      {error ? <p className="text-xs text-destructive">{error}</p> : null}
    </div>
  );
}
