import { Fragment, type ReactNode } from 'react';
import { Zap } from 'lucide-react';
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/Tooltip';
import { defineMessages, useIntl } from '../i18n';
import { cn } from '../utils';
import type { MessageUsage } from '../types/message';
import {
  formatCost,
  formatDuration,
  formatTokenCount,
  formatTokensPerSecond,
  tokensPerSecond,
} from '../utils/usageFormatting';

const i18n = defineMessages({
  tokensPerSecondUnit: {
    id: 'messageUsageStats.tokensPerSecondUnit',
    defaultMessage: 'tok/s',
  },
  tokenUnit: {
    id: 'messageUsageStats.tokenUnit',
    defaultMessage: 'tok',
  },
  input: {
    id: 'messageUsageStats.input',
    defaultMessage: 'Input',
  },
  cacheRead: {
    id: 'messageUsageStats.cacheRead',
    defaultMessage: 'cache read',
  },
  cacheWrite: {
    id: 'messageUsageStats.cacheWrite',
    defaultMessage: 'cache write',
  },
  cacheHitRate: {
    id: 'messageUsageStats.cacheHitRate',
    defaultMessage: '({percent}% of input)',
  },
  output: {
    id: 'messageUsageStats.output',
    defaultMessage: 'Output',
  },
  total: {
    id: 'messageUsageStats.total',
    defaultMessage: 'Total',
  },
  firstToken: {
    id: 'messageUsageStats.firstToken',
    defaultMessage: 'First token',
  },
  totalTime: {
    id: 'messageUsageStats.totalTime',
    defaultMessage: 'Total time',
  },
  speed: {
    id: 'messageUsageStats.speed',
    defaultMessage: 'Speed',
  },
  cost: {
    id: 'messageUsageStats.cost',
    defaultMessage: 'Cost',
  },
  estimated: {
    id: 'messageUsageStats.estimated',
    defaultMessage: '(estimated)',
  },
  reported: {
    id: 'messageUsageStats.reported',
    defaultMessage: '(reported)',
  },
  compaction: {
    id: 'messageUsageStats.compaction',
    defaultMessage: 'Compaction',
  },
});

function hasValue(value: number | null | undefined): value is number {
  return typeof value === 'number' && Number.isFinite(value);
}

/** One label/value row of the tooltip's stats card. */
function StatRow({
  label,
  value,
  sub,
}: {
  label: string;
  value: ReactNode;
  /** Dimmer, indented breakdown row (e.g. cache read/write). */
  sub?: boolean;
}) {
  return (
    <div className={cn('flex items-baseline justify-between gap-8', sub && 'pl-3')}>
      <span className={cn('text-text-inverse/55', sub && 'text-text-inverse/40')}>{label}</span>
      <span
        className={cn(
          'font-mono tabular-nums text-right',
          sub ? 'text-text-inverse/60' : 'text-text-inverse/90'
        )}
      >
        {value}
      </span>
    </div>
  );
}

/**
 * Per-message usage chip (tok/s, cost, total tokens) with a tooltip breaking
 * down tokens, caching, timing, and cost. Renders nothing without data.
 */
export default function MessageUsageStats({ usage }: { usage: MessageUsage }) {
  const intl = useIntl();
  const {
    inputTokens,
    outputTokens,
    totalTokens,
    cacheReadTokens,
    cacheWriteTokens,
    cost,
    costSource,
    elapsedMs,
    timeToFirstTokenMs,
    isCompaction,
  } = usage;

  const tps = tokensPerSecond(outputTokens, elapsedMs);

  const chipSegments: ReactNode[] = [];
  if (tps !== null) {
    chipSegments.push(
      <span key="tps" className="flex items-center gap-1">
        <Zap className="h-3 w-3" aria-hidden="true" />
        {formatTokensPerSecond(tps)} {intl.formatMessage(i18n.tokensPerSecondUnit)}
      </span>
    );
  }
  if (hasValue(cost)) {
    chipSegments.push(
      <span key="cost">
        {costSource === 'estimated' ? '~' : ''}
        {formatCost(cost)}
      </span>
    );
  }
  if (hasValue(totalTokens)) {
    chipSegments.push(
      <span key="tokens">
        {formatTokenCount(totalTokens)} {intl.formatMessage(i18n.tokenUnit)}
      </span>
    );
  }

  if (chipSegments.length === 0) {
    return null;
  }

  const hasTokensSection =
    hasValue(inputTokens) ||
    hasValue(outputTokens) ||
    hasValue(totalTokens) ||
    hasValue(cacheReadTokens) ||
    hasValue(cacheWriteTokens);
  const hasTimingSection = hasValue(timeToFirstTokenMs) || hasValue(elapsedMs) || tps !== null;
  const hasCostSection = hasValue(cost);

  const section = 'py-1.5 first:pt-0 last:pb-0 space-y-[3px]';

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div className="flex items-center gap-1 text-xs font-mono text-text-secondary hover:text-text-primary transition-colors cursor-default">
          {chipSegments.map((segment, index) => (
            <Fragment key={index}>
              {index > 0 && <span className="text-text-tertiary">·</span>}
              {segment}
            </Fragment>
          ))}
        </div>
      </TooltipTrigger>
      <TooltipContent side="top" align="end" className="px-3.5 py-2.5 rounded-lg text-left">
        <div className="min-w-44 text-xs divide-y divide-text-inverse/10">
          {hasTokensSection && (
            <div className={section}>
              {hasValue(inputTokens) && (
                <StatRow
                  label={intl.formatMessage(i18n.input)}
                  value={formatTokenCount(inputTokens)}
                />
              )}
              {hasValue(cacheReadTokens) && (
                <StatRow
                  sub
                  label={intl.formatMessage(i18n.cacheRead)}
                  value={
                    <>
                      {formatTokenCount(cacheReadTokens)}
                      {hasValue(inputTokens) && inputTokens > 0 && (
                        <span className="text-text-inverse/40">
                          {' '}
                          {intl.formatMessage(i18n.cacheHitRate, {
                            percent: Math.round((cacheReadTokens / inputTokens) * 100),
                          })}
                        </span>
                      )}
                    </>
                  }
                />
              )}
              {hasValue(cacheWriteTokens) && (
                <StatRow
                  sub
                  label={intl.formatMessage(i18n.cacheWrite)}
                  value={formatTokenCount(cacheWriteTokens)}
                />
              )}
              {hasValue(outputTokens) && (
                <StatRow
                  label={intl.formatMessage(i18n.output)}
                  value={formatTokenCount(outputTokens)}
                />
              )}
              {hasValue(totalTokens) && (
                <StatRow
                  label={intl.formatMessage(i18n.total)}
                  value={formatTokenCount(totalTokens)}
                />
              )}
            </div>
          )}

          {hasTimingSection && (
            <div className={section}>
              {hasValue(timeToFirstTokenMs) && (
                <StatRow
                  label={intl.formatMessage(i18n.firstToken)}
                  value={formatDuration(timeToFirstTokenMs)}
                />
              )}
              {hasValue(elapsedMs) && (
                <StatRow
                  label={intl.formatMessage(i18n.totalTime)}
                  value={formatDuration(elapsedMs)}
                />
              )}
              {tps !== null && (
                <StatRow
                  label={intl.formatMessage(i18n.speed)}
                  value={`${formatTokensPerSecond(tps)} ${intl.formatMessage(i18n.tokensPerSecondUnit)}`}
                />
              )}
            </div>
          )}

          {hasCostSection && (
            <div className={section}>
              {hasValue(cost) && (
                <StatRow
                  label={intl.formatMessage(i18n.cost)}
                  value={
                    <>
                      {formatCost(cost)}
                      {(costSource === 'provider_reported' || costSource === 'estimated') && (
                        <span className="text-text-inverse/60">
                          {' '}
                          {intl.formatMessage(
                            costSource === 'provider_reported' ? i18n.reported : i18n.estimated
                          )}
                        </span>
                      )}
                    </>
                  }
                />
              )}
            </div>
          )}

          {isCompaction && (
            <div className={cn(section, 'text-amber-300/90')}>
              {intl.formatMessage(i18n.compaction)}
            </div>
          )}
        </div>
      </TooltipContent>
    </Tooltip>
  );
}
