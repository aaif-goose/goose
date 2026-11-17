import { Info, RefreshCw } from 'lucide-react';
import { defineMessages, useIntl } from '../i18n';
import { Button } from './ui/button';
import { cn } from '../utils';

const i18n = defineMessages({
  notice: {
    id: 'subagentSessionNotice.notice',
    defaultMessage: "Subagent progress isn't shown live here.",
  },
  loadLatestProgress: {
    id: 'subagentSessionNotice.loadLatestProgress',
    defaultMessage: 'Load latest progress',
  },
});

interface SubagentSessionNoticeProps {
  refreshing: boolean;
  onRefresh: () => void;
}

export function SubagentSessionNotice({ refreshing, onRefresh }: SubagentSessionNoticeProps) {
  const intl = useIntl();

  return (
    <div
      role="status"
      className="relative z-30 mx-4 mb-4 flex items-center justify-between gap-4 rounded-2xl border border-border-primary bg-background-secondary px-4 py-3"
    >
      <span className="flex items-center gap-2 text-sm text-text-primary">
        <Info className="size-4 shrink-0 text-text-secondary" />
        {intl.formatMessage(i18n.notice)}
      </span>
      <Button variant="default" size="sm" onClick={onRefresh} disabled={refreshing}>
        <RefreshCw className={cn(refreshing && 'animate-spin')} />
        {intl.formatMessage(i18n.loadLatestProgress)}
      </Button>
    </div>
  );
}
