import { useState, useEffect } from 'react';
import { Download, Trash2, X, Check, ChevronDown, ChevronUp } from 'lucide-react';
import { Button } from '../../ui/button';
import { useConfig } from '../../ConfigContext';
import {
  listModels,
  downloadModel,
  getDownloadProgress,
  cancelDownload as cancelDownloadApi,
  deleteModel as deleteModelApi,
  type WhisperModelResponse,
  type DownloadProgress,
} from '../../../api';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  gpuAcceleration: {
    id: 'localModelManager.gpuAcceleration',
    defaultMessage:
      'Supports GPU acceleration (CUDA for NVIDIA, Metal for Apple Silicon). GPU features must be enabled at build time for hardware acceleration.',
  },
  recommended: {
    id: 'localModelManager.recommended',
    defaultMessage: 'Recommended',
  },
  active: {
    id: 'localModelManager.active',
    defaultMessage: 'Active',
  },
  recommendedForHardware: {
    id: 'localModelManager.recommendedForHardware',
    defaultMessage: 'Recommended for your hardware',
  },
  downloaded: {
    id: 'localModelManager.downloaded',
    defaultMessage: 'Downloaded',
  },
  download: {
    id: 'localModelManager.download',
    defaultMessage: 'Download',
  },
  deleteConfirm: {
    id: 'localModelManager.deleteConfirm',
    defaultMessage: 'Delete this model? You can re-download it later.',
  },
  showRecommendedOnly: {
    id: 'localModelManager.showRecommendedOnly',
    defaultMessage: 'Show recommended only',
  },
  showAllModels: {
    id: 'localModelManager.showAllModels',
    defaultMessage: 'Show all models ({count} more)',
  },
  noModels: {
    id: 'localModelManager.noModels',
    defaultMessage: 'No models available',
  },
});

const LOCAL_WHISPER_MODEL_CONFIG_KEY = 'LOCAL_WHISPER_MODEL';

const formatBytes = (bytes: number): string => {
  if (bytes < 1024) return `${bytes}B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)}KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(0)}MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)}GB`;
};

const capitalize = (str: string): string => {
  return str.charAt(0).toUpperCase() + str.slice(1);
};

export const LocalModelManager = () => {
  const intl = useIntl();
  const [models, setModels] = useState<WhisperModelResponse[]>([]);
  const [downloads, setDownloads] = useState<Map<string, DownloadProgress>>(new Map());
  const [selectedModelId, setSelectedModelId] = useState<string | null>(null);
  const [showAllModels, setShowAllModels] = useState(false);
  const { read, upsert } = useConfig();

  useEffect(() => {
    loadModels();
    loadSelectedModel();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const loadSelectedModel = async () => {
    try {
      const value = await read(LOCAL_WHISPER_MODEL_CONFIG_KEY, false);
      if (value && typeof value === 'string') {
        setSelectedModelId(value);
      } else {
        setSelectedModelId(null);
      }
    } catch (error) {
      console.error('Failed to load selected model:', error);
      setSelectedModelId(null);
    }
  };

  const selectModel = async (modelId: string) => {
    await upsert(LOCAL_WHISPER_MODEL_CONFIG_KEY, modelId, false);
    setSelectedModelId(modelId);
  };

  const loadModels = async () => {
    try {
      const response = await listModels();
      if (response.data) {
        setModels(response.data);
      }
    } catch (error) {
      console.error('Failed to load models:', error);
    }
  };

  const startDownload = async (modelId: string) => {
    try {
      await downloadModel({ path: { model_id: modelId } });
      pollDownloadProgress(modelId);
    } catch (error) {
      console.error('Failed to start download:', error);
    }
  };

  const pollDownloadProgress = (modelId: string) => {
    const interval = setInterval(async () => {
      try {
        const response = await getDownloadProgress({ path: { model_id: modelId } });
        if (response.data) {
          const progress = response.data;
          setDownloads((prev) => new Map(prev).set(modelId, progress));

          if (progress.status === 'completed') {
            clearInterval(interval);
            await loadModels(); // Refresh model list
            // Backend auto-selects, but also update frontend state
            await loadSelectedModel();
          } else if (progress.status === 'failed') {
            clearInterval(interval);
            await loadModels();
          }
        } else {
          clearInterval(interval);
        }
      } catch {
        clearInterval(interval);
      }
    }, 500);
  };

  const cancelDownload = async (modelId: string) => {
    try {
      await cancelDownloadApi({ path: { model_id: modelId } });
      setDownloads((prev) => {
        const next = new Map(prev);
        next.delete(modelId);
        return next;
      });
      loadModels();
    } catch (error) {
      console.error('Failed to cancel download:', error);
    }
  };

  const deleteModel = async (modelId: string) => {
    if (!window.confirm(intl.formatMessage(i18n.deleteConfirm))) return;

    try {
      await deleteModelApi({ path: { model_id: modelId } });
      if (selectedModelId === modelId) {
        await upsert(LOCAL_WHISPER_MODEL_CONFIG_KEY, '', false);
        setSelectedModelId(null);
      }
      loadModels();
    } catch (error) {
      console.error('Failed to delete model:', error);
    }
  };

  const hasDownloadedNonRecommended = models.some(
    (model) => model.downloaded && !model.recommended
  );
  const displayedModels =
    showAllModels || hasDownloadedNonRecommended ? models : models.filter((m) => m.recommended);
  const hasNonRecommendedModels = models.some((m) => !m.recommended);
  const showToggleButton = hasNonRecommendedModels && !hasDownloadedNonRecommended;

  return (
    <div className="space-y-3">
      <div className="text-xs text-text-secondary mb-2">
        <p>
          {intl.formatMessage(i18n.gpuAcceleration)}
        </p>
      </div>

      <div className="space-y-2">
        {displayedModels.map((model) => {
          const progress = downloads.get(model.id);
          const isDownloading = progress?.status === 'downloading';
          const isSelected = selectedModelId === model.id;
          const canSelect = model.downloaded && !isDownloading;

          return (
            <div
              key={model.id}
              className={`border rounded-lg p-3 transition-colors ${
                isSelected
                  ? 'border-text-inverse bg-background-inverse/5'
                  : 'border-border-primary bg-background-primary hover:border-border-primary'
              }`}
            >
              <div className="flex items-start justify-between gap-3">
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    {canSelect && (
                      <input
                        type="radio"
                        checked={isSelected}
                        onChange={() => selectModel(model.id)}
                        className="cursor-pointer"
                      />
                    )}
                    <h4 className="text-sm font-medium text-text-primary">
                      {capitalize(model.id)}
                    </h4>
                    <span className="text-xs text-text-secondary">{model.size_mb}MB</span>
                    {model.recommended && (
                      <span className="text-xs bg-blue-500 text-white px-2 py-0.5 rounded">
                        {intl.formatMessage(i18n.recommended)}
                      </span>
                    )}
                    {isSelected && (
                      <span className="text-xs bg-background-inverse text-white px-2 py-0.5 rounded">
                        {intl.formatMessage(i18n.active)}
                      </span>
                    )}
                  </div>

                  <p className="text-xs text-text-secondary mt-1">{model.description}</p>
                  {model.recommended && (
                    <p className="text-xs text-blue-600 mt-1 font-medium">
                      {intl.formatMessage(i18n.recommendedForHardware)}
                    </p>
                  )}
                </div>

                <div className="flex items-center gap-2">
                  {model.downloaded ? (
                    <>
                      <div className="flex items-center gap-1 text-xs text-green-600">
                        <Check className="w-4 h-4" />
                        <span>{intl.formatMessage(i18n.downloaded)}</span>
                      </div>
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => deleteModel(model.id)}
                        className="text-destructive hover:text-destructive"
                      >
                        <Trash2 className="w-4 h-4" />
                      </Button>
                    </>
                  ) : isDownloading ? (
                    <>
                      <div className="text-xs text-text-secondary min-w-[60px]">
                        {progress.progress_percent.toFixed(0)}%
                      </div>
                      <Button variant="ghost" size="sm" onClick={() => cancelDownload(model.id)}>
                        <X className="w-4 h-4" />
                      </Button>
                    </>
                  ) : (
                    <Button variant="outline" size="sm" onClick={() => startDownload(model.id)}>
                      <Download className="w-4 h-4 mr-1" />
                      {intl.formatMessage(i18n.download)}
                    </Button>
                  )}
                </div>
              </div>

              {isDownloading && progress && (
                <div className="mt-2 space-y-1">
                  <div className="w-full bg-background-secondary rounded-full h-1.5">
                    <div
                      className="bg-background-inverse h-1.5 rounded-full transition-all"
                      style={{ width: `${progress.progress_percent}%` }}
                    />
                  </div>
                  <div className="flex justify-between text-xs text-text-secondary">
                    <span>
                      {formatBytes(progress.bytes_downloaded)} / {formatBytes(progress.total_bytes)}
                    </span>
                    {progress.speed_bps && <span>{formatBytes(progress.speed_bps)}/s</span>}
                  </div>
                </div>
              )}

              {progress?.status === 'failed' && progress.error && (
                <div className="mt-2 text-xs text-destructive">{progress.error}</div>
              )}
            </div>
          );
        })}
      </div>

      {showToggleButton && (
        <Button
          variant="ghost"
          size="sm"
          onClick={() => setShowAllModels(!showAllModels)}
          className="w-full text-text-secondary hover:text-text-primary"
        >
          {showAllModels ? (
            <>
              <ChevronUp className="w-4 h-4 mr-1" />
              {intl.formatMessage(i18n.showRecommendedOnly)}
            </>
          ) : (
            <>
              <ChevronDown className="w-4 h-4 mr-1" />
              {intl.formatMessage(i18n.showAllModels, { count: models.length - displayedModels.length })}
            </>
          )}
        </Button>
      )}

      {models.length === 0 && (
        <div className="text-center py-6 text-text-secondary text-sm">{intl.formatMessage(i18n.noModels)}</div>
      )}
    </div>
  );
};
