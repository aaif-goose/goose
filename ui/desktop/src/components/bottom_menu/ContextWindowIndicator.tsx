import { defineMessages, useIntl } from '../../i18n';
import { formatTokenCount } from '../../utils/usageFormatting';

interface ContextWindowIndicatorProps {
  totalTokens: number;
  tokenLimit: number;
  onOpen?: () => void;
}

const i18n = defineMessages({
  open: {
    id: 'contextWindowIndicator.open',
    defaultMessage: '{used} of {limit} tokens used. Show what is in the context window',
  },
});

const getTextColor = (percentage: number): string => {
  if (percentage <= 75) return 'text-text-primary/70';
  if (percentage <= 90) return 'text-orange-500';
  return 'text-red-500';
};

export function ContextWindowIndicator({
  totalTokens,
  tokenLimit,
  onOpen,
}: ContextWindowIndicatorProps) {
  const intl = useIntl();
  if (!tokenLimit) return null;

  const percentage = Math.round((totalTokens / tokenLimit) * 100);
  const used = formatTokenCount(totalTokens);
  const limit = formatTokenCount(tokenLimit);
  const content = (
    <span className={`text-xs font-mono ${getTextColor(percentage)}`}>
      {used} / {limit}
    </span>
  );

  if (!onOpen) {
    return <div className="flex h-full items-center">{content}</div>;
  }

  return (
    <button
      type="button"
      aria-label={intl.formatMessage(i18n.open, { used, limit })}
      className="flex h-full cursor-pointer items-center rounded px-1 hover:bg-background-secondary"
      onClick={onOpen}
    >
      {content}
    </button>
  );
}
