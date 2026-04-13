import { useState, useEffect, useCallback, useRef } from 'react';
import { Download, Trash2, X, ChevronDown, ChevronUp, Settings2, Eye } from 'lucide-react';
import { Button } from '../../ui/button';
import { useModelAndProvider } from '../../ModelAndProviderContext';
import { defineMessages, useIntl } from '../../../i18n';
import {
  listLocalModels,
  downloadHfModel,
  getLocalModelDownloadProgress,
  cancelLocalModelDownload,
  deleteLocalModel,
  setConfigProvider,
  type DownloadProgress,
  type LocalModelResponse,
} from '../../../api';
import { HuggingFaceModelSearch } from './HuggingFaceModelSearch';
import { ModelSettingsPanel } from './ModelSettingsPanel';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '../../ui/dialog';

const i18n = defineMessages({
  title: {
    id: 'localInferenceSettings.title',
    defaultMessage: 'Local Inference Models',
  },
  description: {
    id: 'localInferenceSettings.description',
    defaultMessage:
      'Download and manage local LLM models for inference without API keys. Search HuggingFace for any GGUF model or use the featured picks below.',
  },
  downloading: {
    id: 'localInferenceSettings.downloading',
    defaultMessage: 'Downloading',
  },
  downloadedModels: {
    id: 'localInferenceSettings.downloadedModels',
    defaultMessage: 'Downloaded Models',
  },
  featuredModels: {
    id: 'localInferenceSettings.featuredModels',
    defaultMessage: 'Featured Models',
  },
  recommended: {
    id: 'localInferenceSettings.recommended',
    defaultMessage: 'Recommended',
  },
  download: {
    id: 'localInferenceSettings.download',
    defaultMessage: 'Download',
  },
  showRecommendedOnly: {
    id: 'localInferenceSettings.showRecommendedOnly',
    defaultMessage: 'Show recommended only',
  },
  showAllFeatured: {
    id: 'localInferenceSettings.showAllFeatured',
    defaultMessage: 'Show all featured ({count} more)',
  },
  modelSettings: {
    id: 'localInferenceSettings.modelSettings',
    defaultMessage: 'Model Settings',
  },
  noModels: {
    id: 'localInferenceSettings.noModels',
    defaultMessage: 'No models available',
  },
  downloadProgress: {
    id: 'localInferenceSettings.downloadProgress',
    defaultMessage: '{downloaded} / {total} ({percent}%)',
  },
  remaining: {
    id: 'localInferenceSettings.remaining',
    defaultMessage: '{time} remaining',
  },
  downloadFailed: {
    id: 'localInferenceSettings.downloadFailed',
    defaultMessage: 'Download failed',
  },
  deleteConfirm: {
    id: 'localInferenceSettings.deleteConfirm',
    defaultMessage: 'Delete this model? You can re-download it later.',
  },
  modelSettingsTitle: {
    id: 'localInferenceSettings.modelSettingsTitle',
    defaultMessage: 'Model settings',
  },
  vision: {
    id: 'localInferenceSettings.vision',
    defaultMessage: 'Vision',
  },
  visionEncoderDownloading: {
    id: 'localInferenceSettings.visionEncoderDownloading',
    defaultMessage: 'Vision encoder downloading…',
  },
  visionEncoderNotDownloaded: {
    id: 'localInferenceSettings.visionEncoderNotDownloaded',
    defaultMessage: 'Vision encoder not downloaded',
  },
});

const VisionBadge = ({ model, intl }: { model: LocalModelResponse; intl: ReturnType<typeof useIntl> }) => {
  if (!model.vision_capable) return null;

  const mmproj = model.mmproj_status;
  const isDownloaded = mmproj?.state === 'Downloaded';
  const isDownloading = mmproj?.state === 'Downloading';

  if (isDownloaded) {
    return (
      <span className="inline-flex items-center gap-1 text-xs text-green-400 bg-green-500/10 px-2 py-0.5 rounded">
        <Eye className="w-3 h-3" />
        {intl.formatMessage(i18n.vision)}
      </span>
    );
  }

  if (isDownloading) {
    const percent = mmproj && 'progress_percent' in mmproj
      ? Math.round(mmproj.progress_percent)
      : null;
    return (
      <span className="inline-flex items-center gap-1 text-xs text-yellow-400 bg-yellow-500/10 px-2 py-0.5 rounded">
        <Eye className="w-3 h-3" />
        {intl.formatMessage(i18n.visionEncoderDownloading)}
        {percent != null && ` ${percent}%`}
      </span>
    );
  }

  return (
    <span className="inline-flex items-center gap-1 text-xs text-text-muted bg-background-subtle px-2 py-0.5 rounded">
      <Eye className="w-3 h-3" />
      {intl.formatMessage(i18n.vision)}
    </span>
  );
};

const formatBytes = (bytes: number): string => {
  if (bytes < 1024) return `${bytes}B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)}KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(0)}MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)}GB`;
};

export const LocalInferenceSettings = () => {
  const intl = useIntl();
  const [models, setModels] = useState<LocalModelResponse[]>([]);
  const [downloads, setDownloads] = useState<Map<string, DownloadProgress>>(new Map());
  const [showAllFeatured, setShowAllFeatured] = useState(false);
  const [settingsOpenFor, setSettingsOpenFor] = useState<string | null>(null);
  const { currentModel, currentProvider, refreshCurrentModelAndProvider } = useModelAndProvider();
  const downloadSectionRef = useRef<HTMLDivElement>(null);
  const activePolls = useRef(new Set<string>());
  const selectedModelId = currentProvider === 'local' ? currentModel : null;

  const loadModels = useCallback(async (): Promise<LocalModelResponse[] | undefined> => {
    try {
      const response = await listLocalModels();
      if (response.data) {
        setModels(response.data);
        response.data.forEach((model) => {
          if (model.status.state === 'Downloading') {
            pollDownloadProgress(model.id);
          }
        });

        return response.data;
      }
    } catch (error) {
      console.error('Failed to load models:', error);
    }
    return undefined;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    loadModels();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Poll model list while any vision encoder is downloading
  useEffect(() => {
    const hasDownloadingMmproj = models.some(
      (m) => m.vision_capable && m.mmproj_status?.state === 'Downloading'
    );
    if (!hasDownloadingMmproj) return;

    const interval = setInterval(() => {
      loadModels();
    }, 2000);
    return () => clearInterval(interval);
  }, [models, loadModels]);

  const selectModel = async (modelId: string) => {
    try {
      await setConfigProvider({
        body: { provider: 'local', model: modelId },
        throwOnError: true,
      });
      await refreshCurrentModelAndProvider();
    } catch (error) {
      console.error('Failed to select model:', error);
    }
  };

  const startFeaturedDownload = async (modelId: string) => {
    const model = models.find((m) => m.id === modelId);
    if (!model) return;
    try {
      await downloadHfModel({ body: { spec: model.id } });
      pollDownloadProgress(modelId);
      scrollToDownloads();
    } catch (error) {
      console.error('Failed to start download:', error);
    }
  };

  const scrollToDownloads = useCallback(() => {
    requestAnimationFrame(() => {
      downloadSectionRef.current?.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
    });
  }, []);

  const pollDownloadProgress = (modelId: string) => {
    if (activePolls.current.has(modelId)) return;
    activePolls.current.add(modelId);

    const stopPolling = (interval: ReturnType<typeof setInterval>) => {
      clearInterval(interval);
      activePolls.current.delete(modelId);
    };

    const interval = setInterval(async () => {
      try {
        const response = await getLocalModelDownloadProgress({ path: { model_id: modelId } });
        if (response.data) {
          const progress = response.data;
          setDownloads((prev) => new Map(prev).set(modelId, progress));

          if (progress.status === 'completed') {
            stopPolling(interval);
            setDownloads((prev) => {
              const next = new Map(prev);
              next.delete(modelId);
              return next;
            });
            await loadModels();
            await selectModel(modelId);
          } else if (progress.status === 'failed' || progress.status === 'cancelled') {
            stopPolling(interval);
            setDownloads((prev) => {
              const next = new Map(prev);
              next.delete(modelId);
              return next;
            });
            await loadModels();
          }
        } else {
          stopPolling(interval);
        }
      } catch {
        stopPolling(interval);
      }
    }, 1000);
  };

  const cancelDownload = async (modelId: string) => {
    try {
      await cancelLocalModelDownload({ path: { model_id: modelId } });
      setDownloads((prev) => {
        const next = new Map(prev);
        next.delete(modelId);
        return next;
      });
      await loadModels();
    } catch (error) {
      console.error('Failed to cancel download:', error);
    }
  };

  const handleDeleteModel = async (modelId: string) => {
    if (!window.confirm(intl.formatMessage(i18n.deleteConfirm))) return;
    try {
      await deleteLocalModel({ path: { model_id: modelId } });
      const updatedModels = await loadModels();

      if (selectedModelId === modelId && updatedModels) {
        const remainingDownloaded = updatedModels.filter(
          (m) => m.id !== modelId && m.status.state === 'Downloaded'
        );
        if (remainingDownloaded.length > 0) {
          selectModel(remainingDownloaded[0].id);
        }
      }
    } catch (error) {
      console.error('Failed to delete model:', error);
    }
  };

  const handleHfDownloadStarted = (modelId: string) => {
    pollDownloadProgress(modelId);
    loadModels();
    scrollToDownloads();
  };

  const isDownloaded = (model: LocalModelResponse) => model.status.state === 'Downloaded';
  const isNotDownloaded = (model: LocalModelResponse) =>
    model.status.state === 'NotDownloaded' && !downloads.has(model.id);

  const downloadedModels = models.filter(isDownloaded);
  const notDownloadedModels = models.filter(isNotDownloaded);
  const recommendedModels = notDownloadedModels.filter((m) => m.recommended);
  const displayedFeatured = showAllFeatured ? notDownloadedModels : recommendedModels;
  const showFeaturedToggle = notDownloadedModels.length > recommendedModels.length;

  return (
    <div className="space-y-6">
      <div>
        <h3 className="text-text-default font-medium">{intl.formatMessage(i18n.title)}</h3>
        <p className="text-xs text-text-muted max-w-2xl mt-1">
          {intl.formatMessage(i18n.description)}
        </p>
      </div>

      {/* Active Downloads */}
      {downloads.size > 0 && (
        <div ref={downloadSectionRef}>
          <h4 className="text-sm font-medium text-text-default mb-2">{intl.formatMessage(i18n.downloading)}</h4>
          <div className="space-y-2">
            {Array.from(downloads.entries()).map(([modelId, progress]) => {
              if (progress.status === 'completed') return null;
              return (
                <div
                  key={modelId}
                  className="border rounded-lg p-3 border-border-subtle bg-background-default"
                >
                  <div className="flex items-center justify-between mb-2">
                    <span className="text-sm font-medium text-text-default truncate">
                      {modelId}
                    </span>
                    {progress.status === 'downloading' && (
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => cancelDownload(modelId)}
                        className="text-destructive hover:text-destructive"
                      >
                        <X className="w-4 h-4" />
                      </Button>
                    )}
                  </div>
                  {progress.status === 'downloading' && (
                    <div className="space-y-1">
                      <div className="w-full bg-gray-700 rounded-full h-2">
                        <div
                          className="bg-blue-500 h-2 rounded-full transition-all duration-300"
                          style={{ width: `${progress.progress_percent}%` }}
                        />
                      </div>
                      <div className="flex justify-between text-xs text-text-muted">
                        <span>
                          {intl.formatMessage(i18n.downloadProgress, {
                            downloaded: formatBytes(progress.bytes_downloaded),
                            total: formatBytes(progress.total_bytes),
                            percent: progress.progress_percent.toFixed(0),
                          })}
                        </span>
                        <span className="flex gap-2">
                          {progress.eta_seconds != null && progress.eta_seconds > 0 && (
                            <span>
                              {intl.formatMessage(i18n.remaining, {
                                time:
                                  progress.eta_seconds < 60
                                    ? `${Math.round(progress.eta_seconds)}s`
                                    : `${Math.round(progress.eta_seconds / 60)}m`,
                              })}
                            </span>
                          )}
                          {progress.speed_bps != null && progress.speed_bps > 0 && (
                            <span>{formatBytes(progress.speed_bps)}/s</span>
                          )}
                        </span>
                      </div>
                    </div>
                  )}
                  {progress.status === 'failed' && (
                    <p className="text-xs text-destructive">
                      {progress.error || intl.formatMessage(i18n.downloadFailed)}
                    </p>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* Downloaded Models */}
      {downloadedModels.length > 0 && (
        <div>
          <h4 className="text-sm font-medium text-text-default mb-2">{intl.formatMessage(i18n.downloadedModels)}</h4>
          <div className="space-y-2">
            {downloadedModels.map((model) => {
              const isSelected = selectedModelId === model.id;
              return (
                <div
                  key={model.id}
                  className={`border rounded-lg p-3 transition-colors ${
                    isSelected
                      ? 'border-accent-primary bg-accent-primary/5'
                      : 'border-border-subtle bg-background-default hover:border-border-default'
                  }`}
                >
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <input
                        type="radio"
                        checked={isSelected}
                        onChange={() => selectModel(model.id)}
                        className="cursor-pointer"
                      />
                      <span className="text-sm font-medium text-text-default">{model.id}</span>
                      <span className="text-xs text-text-muted">
                        {formatBytes(model.size_bytes)}
                      </span>
                      {model.recommended && (
                        <span className="text-xs bg-blue-500 text-white px-2 py-0.5 rounded">
                          {intl.formatMessage(i18n.recommended)}
                        </span>
                      )}
                      <VisionBadge model={model} intl={intl} />
                    </div>
                    <div className="flex items-center gap-1">
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => setSettingsOpenFor(model.id)}
                        title={intl.formatMessage(i18n.modelSettingsTitle)}
                      >
                        <Settings2 className="w-4 h-4" />
                      </Button>
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => handleDeleteModel(model.id)}
                        className="text-destructive hover:text-destructive"
                      >
                        <Trash2 className="w-4 h-4" />
                      </Button>
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* Featured Models (not yet downloaded) */}
      {displayedFeatured.length > 0 && (
        <div>
          <h4 className="text-sm font-medium text-text-default mb-2">{intl.formatMessage(i18n.featuredModels)}</h4>
          <div className="space-y-2">
            {displayedFeatured.map((model) => (
              <div
                key={model.id}
                className="border rounded-lg p-3 border-border-subtle bg-background-default hover:border-border-default"
              >
                <div className="flex items-center justify-between gap-3">
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 flex-wrap">
                      <h4 className="text-sm font-medium text-text-default">{model.id}</h4>
                      <span className="text-xs text-text-muted">
                        {formatBytes(model.size_bytes)}
                      </span>
                      {model.recommended && (
                        <span className="text-xs bg-blue-500 text-white px-2 py-0.5 rounded">
                          {intl.formatMessage(i18n.recommended)}
                        </span>
                      )}
                      <VisionBadge model={model} intl={intl} />
                    </div>
                  </div>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => startFeaturedDownload(model.id)}
                  >
                    <Download className="w-4 h-4 mr-1" />
                    {intl.formatMessage(i18n.download)}
                  </Button>
                </div>
              </div>
            ))}
          </div>

          {showFeaturedToggle && (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setShowAllFeatured(!showAllFeatured)}
              className="w-full text-text-muted hover:text-text-default mt-2"
            >
              {showAllFeatured ? (
                <>
                  <ChevronUp className="w-4 h-4 mr-1" />
                  {intl.formatMessage(i18n.showRecommendedOnly)}
                </>
              ) : (
                <>
                  <ChevronDown className="w-4 h-4 mr-1" />
                  {intl.formatMessage(i18n.showAllFeatured, {
                    count: notDownloadedModels.length - displayedFeatured.length,
                  })}
                </>
              )}
            </Button>
          )}
        </div>
      )}

      {/* HuggingFace Search */}
      <div className="border-t border-border-subtle pt-4">
        <HuggingFaceModelSearch onDownloadStarted={handleHfDownloadStarted} />
      </div>

      {models.length === 0 && (
        <div className="text-center py-6 text-text-muted text-sm">{intl.formatMessage(i18n.noModels)}</div>
      )}

      <Dialog
        open={!!settingsOpenFor}
        onOpenChange={(open) => {
          if (!open) setSettingsOpenFor(null);
        }}
      >
        <DialogContent className="max-h-[80vh] overflow-y-auto sm:max-w-xl">
          <DialogHeader>
            <DialogTitle>{intl.formatMessage(i18n.modelSettings)}</DialogTitle>
            <p className="text-sm text-text-muted">{settingsOpenFor || ''}</p>
          </DialogHeader>
          {settingsOpenFor && <ModelSettingsPanel modelId={settingsOpenFor} />}
        </DialogContent>
      </Dialog>
    </div>
  );
};
