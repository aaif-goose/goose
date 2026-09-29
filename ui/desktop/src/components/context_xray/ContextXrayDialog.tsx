import { useCallback, useEffect, useRef, useState } from 'react';
import { RefreshCw, ScrollText } from 'lucide-react';
import { defineMessages, useIntl } from '../../i18n';
import { cn } from '../../utils';
import { getContextReport, type ContextReport } from '../../acp/contextReport';
import { formatPercent, formatTokenCount } from '../../utils/usageFormatting';
import { Button } from '../ui/button';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '../ui/dialog';
import { Skeleton } from '../ui/skeleton';
import { BreakdownList } from './BreakdownList';
import { CapacityMeter } from './CapacityMeter';

const i18n = defineMessages({
  title: {
    id: 'contextXray.title',
    defaultMessage: 'Context window',
  },
  modelSummary: {
    id: 'contextXray.modelSummary',
    defaultMessage: '{model} · {limit} token context window',
  },
  usage: {
    id: 'contextXray.usage',
    defaultMessage: 'of {limit} tokens · {percent} of the context window',
  },
  refresh: {
    id: 'contextXray.refresh',
    defaultMessage: 'Refresh',
  },
  loadError: {
    id: 'contextXray.loadError',
    defaultMessage: 'Could not load the context report.',
  },
  retry: {
    id: 'contextXray.retry',
    defaultMessage: 'Retry',
  },
  tokenizerNote: {
    id: 'contextXray.tokenizerNote',
    defaultMessage: 'Token counts are estimated with the o200k tokenizer.',
  },
  compactNow: {
    id: 'contextXray.compactNow',
    defaultMessage: 'Compact now',
  },
  providerManagesContext: {
    id: 'contextXray.providerManagesContext',
    defaultMessage:
      '{provider} keeps its own conversation history and only receives the latest prompt from goose, so there is no request to break down.',
  },
  autoCompactionPending: {
    id: 'contextXray.autoCompactionPending',
    defaultMessage:
      'Past the auto-compaction threshold: the conversation is compacted before the next request is sent.',
  },
});

function ReportSkeleton() {
  return (
    <div className="flex flex-col gap-6" data-testid="context-report-loading">
      <div className="flex flex-col gap-2">
        <Skeleton className="h-9 w-24" />
        <Skeleton className="h-4 w-56" />
      </div>
      <Skeleton className="h-3 w-full rounded-full" />
      <div className="flex flex-col gap-2">
        {Array.from({ length: 6 }, (_, index) => (
          <Skeleton key={index} className="h-6 w-full" />
        ))}
      </div>
    </div>
  );
}

interface ContextXrayDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  sessionId: string;
  /** Live session total; a change while open means a turn finished, so the report is refetched. */
  totalTokens: number;
  onCompact: () => void;
  compactDisabled: boolean;
}

export function ContextXrayDialog({
  open,
  onOpenChange,
  sessionId,
  totalTokens,
  onCompact,
  compactDisabled,
}: ContextXrayDialogProps) {
  const intl = useIntl();
  const [report, setReport] = useState<ContextReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(false);
  const requestIdRef = useRef(0);

  const fetchReport = useCallback(async () => {
    const requestId = ++requestIdRef.current;
    setLoading(true);
    try {
      const result = await getContextReport(sessionId);
      if (requestId !== requestIdRef.current) return;
      setReport(result);
      setError(false);
    } catch (err) {
      if (requestId !== requestIdRef.current) return;
      console.error('Failed to load context report:', err);
      setReport(null);
      setError(true);
    } finally {
      if (requestId === requestIdRef.current) setLoading(false);
    }
  }, [sessionId]);

  useEffect(() => {
    requestIdRef.current += 1;
    setReport(null);
    setError(false);
    setLoading(false);
  }, [sessionId]);

  useEffect(() => {
    if (open) void fetchReport();
  }, [open, totalTokens, fetchReport]);

  const handleCompact = () => {
    onCompact();
    onOpenChange(false);
  };

  const contextLimit = report?.model.contextLimit ?? 0;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex max-h-[85vh] flex-col sm:max-w-xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            {intl.formatMessage(i18n.title)}
            <Button
              type="button"
              variant="ghost"
              size="xs"
              shape="round"
              onClick={() => void fetchReport()}
              disabled={loading}
              aria-label={intl.formatMessage(i18n.refresh)}
              className="text-text-secondary hover:text-text-primary"
            >
              <RefreshCw className={cn('size-3.5', loading && 'animate-spin')} />
            </Button>
          </DialogTitle>
          <DialogDescription>
            {report
              ? intl.formatMessage(i18n.modelSummary, {
                  model: report.model.modelName,
                  limit: formatTokenCount(contextLimit),
                })
              : ' '}
          </DialogDescription>
        </DialogHeader>
        <div className="min-h-0 flex-1 overflow-y-auto">
          <div className="flex flex-col gap-6">
            {error && (
              <div className="flex items-center gap-3">
                <p className="text-sm text-text-secondary">{intl.formatMessage(i18n.loadError)}</p>
                <Button
                  type="button"
                  variant="secondary"
                  size="sm"
                  onClick={() => void fetchReport()}
                >
                  {intl.formatMessage(i18n.retry)}
                </Button>
              </div>
            )}
            {report?.providerManagesContext ? (
              <p className="text-sm text-text-secondary">
                {intl.formatMessage(i18n.providerManagesContext, {
                  provider: report.model.provider ?? report.model.modelName,
                })}
              </p>
            ) : report ? (
              <>
                <div>
                  <div className="text-3xl font-semibold text-text-primary">
                    {formatTokenCount(report.totalTokens)}
                  </div>
                  <div className="mt-1 text-sm text-text-secondary">
                    {intl.formatMessage(i18n.usage, {
                      limit: formatTokenCount(contextLimit),
                      percent: formatPercent(report.totalTokens, contextLimit),
                    })}
                  </div>
                </div>
                {report.autoCompactionPending && (
                  <p className="text-sm text-orange-500">
                    {intl.formatMessage(i18n.autoCompactionPending)}
                  </p>
                )}
                <CapacityMeter report={report} />
                <BreakdownList report={report} />
                <p className="text-xs text-text-tertiary">
                  {intl.formatMessage(i18n.tokenizerNote)}
                </p>
              </>
            ) : (
              !error && <ReportSkeleton />
            )}
            <div className="flex justify-end">
              <Button
                type="button"
                variant="secondary"
                size="sm"
                shape="pill"
                onClick={handleCompact}
                disabled={compactDisabled}
              >
                <ScrollText className="size-3.5" />
                {intl.formatMessage(i18n.compactNow)}
              </Button>
            </div>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
