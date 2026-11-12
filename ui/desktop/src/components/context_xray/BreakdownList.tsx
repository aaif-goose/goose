import { useState } from 'react';
import { ChevronRight } from 'lucide-react';
import { defineMessages, useIntl } from '../../i18n';
import { cn } from '../../utils';
import type {
  ContextCategory,
  ContextPart,
  ContextReport,
  ContextSegment,
} from '../../acp/contextReport';
import { formatPercent, formatTokenCount } from '../../utils/usageFormatting';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '../ui/collapsible';
import { categoryColorClass, categoryMessages } from './categories';

const i18n = defineMessages({
  free: {
    id: 'contextXray.free',
    defaultMessage: 'Free',
  },
});

function Chevron({ open }: { open: boolean }) {
  return (
    <ChevronRight
      className={cn('size-3 shrink-0 text-text-tertiary transition-transform', open && 'rotate-90')}
    />
  );
}

function ContentPreview({ text }: { text: string }) {
  return (
    <pre className="max-h-48 overflow-auto whitespace-pre-wrap rounded-md bg-background-secondary p-2 font-mono text-xs text-text-secondary">
      {text}
    </pre>
  );
}

function PartRow({ part }: { part: ContextPart }) {
  const [open, setOpen] = useState(false);
  const row = (
    <div className="flex w-full items-center gap-2 py-0.5">
      {part.contentPreview ? <Chevron open={open} /> : <span className="w-3 shrink-0" />}
      <span className="truncate text-xs text-text-primary">{part.label}</span>
      {part.source && <span className="truncate text-xs text-text-tertiary">{part.source}</span>}
      <span className="ml-auto shrink-0 font-mono text-xs text-text-primary/70">
        {formatTokenCount(part.tokenCount)}
      </span>
    </div>
  );

  if (!part.contentPreview) return row;

  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <CollapsibleTrigger asChild>
        <button
          type="button"
          className="-mx-1 w-full cursor-pointer rounded-md px-1 text-left hover:bg-background-secondary"
        >
          {row}
        </button>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="py-1 pl-5">
          <ContentPreview text={part.contentPreview} />
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}

function SegmentRow({
  segment,
  categoryTotal,
  colorClass,
}: {
  segment: ContextSegment;
  categoryTotal: number;
  colorClass: string;
}) {
  const [open, setOpen] = useState(false);
  const parts = segment.parts ?? [];
  const expandable = parts.length > 0 || !!segment.contentPreview;
  const share = categoryTotal > 0 ? Math.min(100, (segment.tokenCount / categoryTotal) * 100) : 0;

  const row = (
    <div className="w-full min-w-0">
      <div className="flex w-full items-center gap-2">
        {expandable ? <Chevron open={open} /> : <span className="w-3 shrink-0" />}
        <span className="truncate text-xs text-text-primary">{segment.label}</span>
        {segment.source && (
          <span className="truncate text-xs text-text-tertiary">{segment.source}</span>
        )}
        <span className="ml-auto shrink-0 font-mono text-xs text-text-primary/70">
          {formatTokenCount(segment.tokenCount)}
        </span>
      </div>
      <div className="ml-5 mt-1 h-1 rounded-full bg-background-tertiary">
        <div className={cn('h-full rounded-full', colorClass)} style={{ width: `${share}%` }} />
      </div>
    </div>
  );

  if (!expandable) return <div className="-mx-1 px-1 py-1">{row}</div>;

  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <CollapsibleTrigger asChild>
        <button
          type="button"
          className="-mx-1 w-full cursor-pointer rounded-md px-1 py-1 text-left hover:bg-background-secondary"
        >
          {row}
        </button>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="flex flex-col py-1 pl-5">
          {parts.map((part, index) => (
            <PartRow key={`${part.label}-${index}`} part={part} />
          ))}
          {parts.length === 0 && segment.contentPreview && (
            <ContentPreview text={segment.contentPreview} />
          )}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}

function CategoryRow({
  category,
  segments,
  contextLimit,
}: {
  category: ContextCategory;
  segments: ContextSegment[];
  contextLimit: number;
}) {
  const intl = useIntl();
  const [open, setOpen] = useState(false);
  const total = segments.reduce((sum, segment) => sum + segment.tokenCount, 0);
  const colorClass = categoryColorClass[category];

  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <CollapsibleTrigger asChild>
        <button
          type="button"
          className="-mx-1 flex w-full cursor-pointer items-center gap-2 rounded-md px-1 py-1.5 text-left hover:bg-background-secondary"
        >
          <span className={cn('h-2.5 w-2.5 shrink-0 rounded-full', colorClass)} />
          <span className="min-w-0 flex-1 truncate text-sm text-text-primary">
            {intl.formatMessage(categoryMessages[category])}
          </span>
          <span className="font-mono text-xs text-text-primary/70">{formatTokenCount(total)}</span>
          <span className="w-10 text-right font-mono text-xs text-text-tertiary">
            {formatPercent(total, contextLimit)}
          </span>
          <Chevron open={open} />
        </button>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="flex flex-col gap-1 pb-2 pl-[18px]">
          {segments.map((segment, index) => (
            <SegmentRow
              key={`${segment.label}-${index}`}
              segment={segment}
              categoryTotal={total}
              colorClass={colorClass}
            />
          ))}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}

export function BreakdownList({ report }: { report: ContextReport }) {
  const intl = useIntl();
  const { contextLimit } = report.model;
  const freeTokens = Math.max(0, contextLimit - report.totalTokens);

  const grouped = new Map<ContextCategory, ContextSegment[]>();
  for (const segment of report.segments) {
    grouped.set(segment.category, [...(grouped.get(segment.category) ?? []), segment]);
  }

  return (
    <div className="flex flex-col">
      {[...grouped].map(([category, segments]) => (
        <CategoryRow
          key={category}
          category={category}
          segments={segments}
          contextLimit={contextLimit}
        />
      ))}
      <div className="-mx-1 flex items-center gap-2 px-1 py-1.5">
        <span className="h-2.5 w-2.5 shrink-0 rounded-full border border-border-primary" />
        <span className="min-w-0 flex-1 truncate text-sm text-text-secondary">
          {intl.formatMessage(i18n.free)}
        </span>
        <span className="font-mono text-xs text-text-primary/70">
          {formatTokenCount(freeTokens)}
        </span>
        <span className="w-10 text-right font-mono text-xs text-text-tertiary">
          {formatPercent(freeTokens, contextLimit)}
        </span>
        <span className="w-3 shrink-0" />
      </div>
    </div>
  );
}
