import { useIntl } from '../../i18n';
import { cn } from '../../utils';
import type { ContextCategory, ContextReport } from '../../acp/contextReport';
import { formatPercent, formatTokenCount } from '../../utils/usageFormatting';
import { Tooltip, TooltipContent, TooltipTrigger } from '../ui/Tooltip';
import { categoryColorClass, categoryMessages } from './categories';

const USED_PORTION_MIN_WIDTH_PX = 16;

export function CapacityMeter({ report }: { report: ContextReport }) {
  const intl = useIntl();
  const { contextLimit } = report.model;
  const usedTokens = report.totalTokens;
  const usedPercent = contextLimit > 0 ? Math.min(100, (usedTokens / contextLimit) * 100) : 0;

  const byCategory = new Map<ContextCategory, number>();
  for (const segment of report.segments) {
    byCategory.set(segment.category, (byCategory.get(segment.category) ?? 0) + segment.tokenCount);
  }
  const bands = [...byCategory].filter(([, tokens]) => tokens > 0);

  return (
    <div className="flex h-3 w-full min-w-0 gap-[2px]">
      {usedTokens > 0 && (
        <div
          data-testid="context-meter-used"
          className="flex h-full gap-[2px]"
          style={{ width: `${usedPercent}%`, minWidth: USED_PORTION_MIN_WIDTH_PX }}
        >
          {bands.map(([category, tokens], index) => {
            const name = intl.formatMessage(categoryMessages[category]);
            const detail = `${formatTokenCount(tokens)} · ${formatPercent(tokens, contextLimit)}`;
            return (
              <Tooltip key={category}>
                <TooltipTrigger asChild>
                  <div
                    role="img"
                    aria-label={`${name}: ${detail}`}
                    className={cn(
                      'h-full',
                      categoryColorClass[category],
                      index === 0 && 'rounded-l-[4px]'
                    )}
                    style={{ width: `${(tokens / usedTokens) * 100}%`, minWidth: 1 }}
                  />
                </TooltipTrigger>
                <TooltipContent side="top">
                  <div className="font-medium">{name}</div>
                  <div>{detail}</div>
                </TooltipContent>
              </Tooltip>
            );
          })}
        </div>
      )}
      <div
        className={cn(
          'h-full min-w-0 flex-1 rounded-r-[4px] bg-background-tertiary',
          usedTokens === 0 && 'rounded-l-[4px]'
        )}
        aria-hidden="true"
      />
    </div>
  );
}
