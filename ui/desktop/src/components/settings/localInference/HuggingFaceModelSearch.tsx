import { useState, useCallback, useRef } from 'react';
import { Search, Download, ChevronDown, ChevronUp, Loader2, Star } from 'lucide-react';
import { Button } from '../../ui/button';
import {
  searchHfModels,
  getRepoFiles,
  downloadHfModel,
  type HfModelInfo,
  type HfQuantVariant,
} from '../../../api';
import { toastError } from '../../../toasts';
import { errorMessage } from '../../../utils/conversionUtils';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  searchHuggingFace: {
    id: 'huggingFaceModelSearch.searchHuggingFace',
    defaultMessage: 'Search HuggingFace',
  },
  searchPlaceholder: {
    id: 'huggingFaceModelSearch.searchPlaceholder',
    defaultMessage: 'Search for GGUF models...',
  },
  loadingVariants: {
    id: 'huggingFaceModelSearch.loadingVariants',
    defaultMessage: 'Loading variants...',
  },
  recommended: {
    id: 'huggingFaceModelSearch.recommended',
    defaultMessage: 'Recommended',
  },
  download: {
    id: 'huggingFaceModelSearch.download',
    defaultMessage: 'Download',
  },
  directDownload: {
    id: 'huggingFaceModelSearch.directDownload',
    defaultMessage: 'Direct Download',
  },
  directDownloadDescription: {
    id: 'huggingFaceModelSearch.directDownloadDescription',
    defaultMessage: 'Specify a model directly: {format}',
  },
  directDownloadFailed: {
    id: 'huggingFaceModelSearch.directDownloadFailed',
    defaultMessage: 'Direct download failed',
  },
  directDownloadErrorMsg: {
    id: 'huggingFaceModelSearch.directDownloadErrorMsg',
    defaultMessage: 'Failed to start the download. Check the spec: {error}',
  },
  noGgufModels: {
    id: 'huggingFaceModelSearch.noGgufModels',
    defaultMessage: 'No GGUF models found for this query.',
  },
  searchError: {
    id: 'huggingFaceModelSearch.searchError',
    defaultMessage: 'Search error: {details}',
  },
  searchNoData: {
    id: 'huggingFaceModelSearch.searchNoData',
    defaultMessage: 'Search returned no data.',
  },
  searchFailed: {
    id: 'huggingFaceModelSearch.searchFailed',
    defaultMessage: 'Search failed. Please try again.',
  },
});

const formatBytes = (bytes: number): string => {
  if (bytes === 0) return 'unknown';
  if (bytes < 1024) return `${bytes}B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)}KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(0)}MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)}GB`;
};

const formatDownloads = (n: number): string => {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return `${n}`;
};

interface RepoData {
  variants: HfQuantVariant[];
  recommendedIndex: number | null;
}

interface Props {
  onDownloadStarted: (modelId: string) => void;
}

export const HuggingFaceModelSearch = ({ onDownloadStarted }: Props) => {
  const intl = useIntl();
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<HfModelInfo[]>([]);
  const [expandedRepo, setExpandedRepo] = useState<string | null>(null);
  const [repoData, setRepoData] = useState<Record<string, RepoData>>({});
  const [searching, setSearching] = useState(false);
  const [downloading, setDownloading] = useState<Set<string>>(new Set());
  const [loadingFiles, setLoadingFiles] = useState<Set<string>>(new Set());
  const [directSpec, setDirectSpec] = useState('');
  const [error, setError] = useState<string | null>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const doSearch = useCallback(async (q: string) => {
    if (!q.trim()) {
      setResults([]);
      setError(null);
      return;
    }
    setSearching(true);
    setError(null);
    try {
      const response = await searchHfModels({
        query: { q, limit: 20 },
      });
      if (response.data) {
        // Pre-fetch variants for all results and filter out repos with no suitable quantizations
        const modelsWithVariants = await Promise.all(
          response.data.map(async (model) => {
            try {
              const [author, repo] = model.repo_id.split('/');
              const filesResponse = await getRepoFiles({ path: { author, repo } });
              if (filesResponse.data && filesResponse.data.variants.length > 0) {
                return { model, data: filesResponse.data };
              }
            } catch {
              // Skip repos we can't fetch
            }
            return null;
          })
        );

        const validResults = modelsWithVariants.filter(Boolean) as {
          model: HfModelInfo;
          data: { variants: HfQuantVariant[]; recommended_index?: number | null };
        }[];

        setResults(validResults.map((r) => r.model));
        setRepoData((prev) => {
          const next = { ...prev };
          for (const r of validResults) {
            next[r.model.repo_id] = {
              variants: r.data.variants,
              recommendedIndex: r.data.recommended_index ?? null,
            };
          }
          return next;
        });

        if (validResults.length === 0) {
          setError(intl.formatMessage(i18n.noGgufModels));
        }
      } else {
        console.error('Search response:', response);
        const errMsg = response.error
          ? intl.formatMessage(i18n.searchError, { details: JSON.stringify(response.error) })
          : intl.formatMessage(i18n.searchNoData);
        setError(errMsg);
      }
    } catch (e) {
      console.error('Search failed:', e);
      setError(intl.formatMessage(i18n.searchFailed));
    } finally {
      setSearching(false);
    }
  }, [intl]);

  const handleQueryChange = (value: string) => {
    setQuery(value);
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => doSearch(value), 300);
  };

  const toggleRepo = async (repoId: string) => {
    if (expandedRepo === repoId) {
      setExpandedRepo(null);
      return;
    }
    setExpandedRepo(repoId);

    if (!repoData[repoId]?.variants.length) {
      setLoadingFiles((prev) => new Set(prev).add(repoId));
      try {
        const [author, repo] = repoId.split('/');
        const response = await getRepoFiles({
          path: { author, repo },
        });
        if (response.data) {
          const variants = response.data.variants;
          setRepoData((prev) => ({
            ...prev,
            [repoId]: {
              variants,
              recommendedIndex: response.data!.recommended_index ?? null,
            },
          }));
        }
      } catch (e) {
        console.error('Failed to fetch repo files:', e);
      } finally {
        setLoadingFiles((prev) => {
          const next = new Set(prev);
          next.delete(repoId);
          return next;
        });
      }
    }
  };

  const startDownload = async (repoId: string, quantization: string) => {
    const spec = `${repoId}:${quantization}`;
    setDownloading((prev) => new Set(prev).add(spec));
    try {
      const response = await downloadHfModel({
        body: { spec },
      });
      if (response.data) {
        onDownloadStarted(response.data);
      }
    } catch (e) {
      console.error('Download failed:', e);
    } finally {
      setDownloading((prev) => {
        const next = new Set(prev);
        next.delete(spec);
        return next;
      });
    }
  };

  const startDirectDownload = async () => {
    const spec = directSpec.trim();
    if (!spec) return;
    const key = `direct:${spec}`;
    setDownloading((prev) => new Set(prev).add(key));
    try {
      const response = await downloadHfModel({
        body: { spec },
        throwOnError: true,
      });
      if (response.data) {
        onDownloadStarted(response.data);
        setDirectSpec('');
      }
    } catch (e) {
      toastError({
        title: intl.formatMessage(i18n.directDownloadFailed),
        msg: intl.formatMessage(i18n.directDownloadErrorMsg, { error: errorMessage(e) }),
      });
    } finally {
      setDownloading((prev) => {
        const next = new Set(prev);
        next.delete(key);
        return next;
      });
    }
  };

  return (
    <div className="space-y-4">
      <div>
        <h4 className="text-sm font-medium text-text-default mb-2">{intl.formatMessage(i18n.searchHuggingFace)}</h4>
        <div className="relative">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-text-muted" />
          <input
            type="text"
            value={query}
            onChange={(e) => handleQueryChange(e.target.value)}
            placeholder={intl.formatMessage(i18n.searchPlaceholder)}
            className="w-full pl-9 pr-4 py-2 text-sm border border-border-subtle rounded-lg bg-background-default text-text-default placeholder:text-text-muted focus:outline-none focus:border-accent-primary"
          />
          {searching && (
            <Loader2 className="absolute right-3 top-1/2 -translate-y-1/2 w-4 h-4 text-text-muted animate-spin" />
          )}
        </div>
      </div>

      {error && !searching && <p className="text-xs text-text-muted">{error}</p>}

      {results.length > 0 && (
        <div className="space-y-1 max-h-96 overflow-y-auto">
          {results.map((model) => {
            const isExpanded = expandedRepo === model.repo_id;
            const data = repoData[model.repo_id];
            const variants = data?.variants || [];
            const recommendedIndex = data?.recommendedIndex ?? null;

            return (
              <div key={model.repo_id} className="border border-border-subtle rounded-lg">
                <button
                  onClick={() => toggleRepo(model.repo_id)}
                  className="w-full flex items-center justify-between p-3 text-left hover:bg-background-subtle rounded-lg"
                >
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-medium text-text-default truncate">
                        {model.repo_id}
                      </span>
                    </div>
                    <div className="flex items-center gap-3 mt-0.5">
                      <span className="text-xs text-text-muted">
                        ↓ {formatDownloads(model.downloads)}
                      </span>
                    </div>
                  </div>
                  {isExpanded ? (
                    <ChevronUp className="w-4 h-4 text-text-muted flex-shrink-0" />
                  ) : (
                    <ChevronDown className="w-4 h-4 text-text-muted flex-shrink-0" />
                  )}
                </button>

                {isExpanded && (
                  <div className="border-t border-border-subtle px-3 pb-3 space-y-1">
                    {loadingFiles.has(model.repo_id) && (
                      <div className="flex items-center gap-2 py-2 text-xs text-text-muted">
                        <Loader2 className="w-3 h-3 animate-spin" />
                        {intl.formatMessage(i18n.loadingVariants)}
                      </div>
                    )}
                    {variants.map((variant, idx) => {
                      const dlKey = `${model.repo_id}:${variant.quantization}`;
                      const isStarting = downloading.has(dlKey);
                      const isRecommended = idx === recommendedIndex;

                      return (
                        <div
                          key={variant.quantization}
                          className={`flex items-center justify-between py-2 px-2 rounded ${
                            isRecommended
                              ? 'bg-blue-500/5 border border-blue-500/20'
                              : 'hover:bg-background-subtle'
                          }`}
                        >
                          <div className="flex flex-col gap-0.5 min-w-0 flex-1 mr-3">
                            <div className="flex items-center gap-2">
                              <span className="text-xs font-mono font-medium text-text-default">
                                {variant.quantization}
                              </span>
                              <span className="text-xs text-text-muted">
                                {formatBytes(variant.size_bytes)}
                              </span>
                              {isRecommended && (
                                <span className="inline-flex items-center gap-1 text-xs bg-blue-500 text-white px-1.5 py-0.5 rounded">
                                  <Star className="w-3 h-3" />
                                  {intl.formatMessage(i18n.recommended)}
                                </span>
                              )}
                            </div>
                            {variant.description && (
                              <span className="text-xs text-text-muted">{variant.description}</span>
                            )}
                          </div>
                          <Button
                            variant="outline"
                            size="sm"
                            disabled={isStarting}
                            onClick={() => startDownload(model.repo_id, variant.quantization)}
                          >
                            {isStarting ? (
                              <Loader2 className="w-3 h-3 animate-spin" />
                            ) : (
                              <>
                                <Download className="w-3 h-3 mr-1" />
                                {intl.formatMessage(i18n.download)}
                              </>
                            )}
                          </Button>
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      <div>
        <h4 className="text-sm font-medium text-text-default mb-2">{intl.formatMessage(i18n.directDownload)}</h4>
        <p className="text-xs text-text-muted mb-2">
          {intl.formatMessage(i18n.directDownloadDescription, {
            format: 'user/repo:quantization',
          })}
        </p>
        <div className="flex gap-2">
          <input
            type="text"
            value={directSpec}
            onChange={(e) => setDirectSpec(e.target.value)}
            placeholder="bartowski/Llama-3.2-1B-Instruct-GGUF:Q4_K_M"
            className="flex-1 px-3 py-2 text-sm border border-border-subtle rounded-lg bg-background-default text-text-default placeholder:text-text-muted focus:outline-none focus:border-accent-primary"
            onKeyDown={(e) => {
              if (e.key === 'Enter') startDirectDownload();
            }}
          />
          <Button
            variant="outline"
            size="sm"
            disabled={!directSpec.trim() || downloading.has(`direct:${directSpec}`)}
            onClick={startDirectDownload}
          >
            {downloading.has(`direct:${directSpec}`) ? (
              <Loader2 className="w-4 h-4 animate-spin" />
            ) : (
              <>
                <Download className="w-4 h-4 mr-1" />
                {intl.formatMessage(i18n.download)}
              </>
            )}
          </Button>
        </div>
      </div>
    </div>
  );
};
