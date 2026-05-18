import { AlertCircle, CheckCircle2, Loader2 } from 'lucide-react';
import { Button } from '../ui/button';
import { defineMessages, useIntl } from '../../i18n';
import type { SyncState } from './useCopilotPrefs';

const i18n = defineMessages({
  loading: {
    id: 'copilotSaveIndicator.loading',
    defaultMessage: 'Loading…',
  },
  syncing: {
    id: 'copilotSaveIndicator.syncing',
    defaultMessage: 'Saving…',
  },
  saved: {
    id: 'copilotSaveIndicator.saved',
    defaultMessage: 'Saved',
  },
  savedPending: {
    id: 'copilotSaveIndicator.savedPending',
    defaultMessage: 'Saved locally — webhook sync pending',
  },
  failed: {
    id: 'copilotSaveIndicator.failed',
    defaultMessage: "Couldn't save",
  },
  retry: {
    id: 'copilotSaveIndicator.retry',
    defaultMessage: 'Retry',
  },
});

export function SaveIndicator({
  syncState,
  onRetry,
}: {
  syncState: SyncState;
  onRetry: () => void;
}) {
  const intl = useIntl();

  if (syncState.kind === 'idle') return null;

  if (syncState.kind === 'loading') {
    return (
      <span className="inline-flex items-center gap-1.5 text-xs text-text-secondary">
        <Loader2 className="h-3 w-3 animate-spin" />
        {intl.formatMessage(i18n.loading)}
      </span>
    );
  }

  if (syncState.kind === 'syncing') {
    return (
      <span className="inline-flex items-center gap-1.5 text-xs text-text-secondary">
        <Loader2 className="h-3 w-3 animate-spin" />
        {intl.formatMessage(i18n.syncing)}
      </span>
    );
  }

  if (syncState.kind === 'synced') {
    if (syncState.switchboardSynced) {
      return (
        <span className="inline-flex items-center gap-1.5 text-xs text-green-700 dark:text-green-400">
          <CheckCircle2 className="h-3 w-3" />
          {intl.formatMessage(i18n.saved)}
        </span>
      );
    }
    return (
      <span className="inline-flex items-center gap-1.5 text-xs text-amber-700 dark:text-amber-400">
        <AlertCircle className="h-3 w-3" />
        {intl.formatMessage(i18n.savedPending)}
        <Button variant="ghost" size="sm" className="h-5 px-2 text-xs" onClick={onRetry}>
          {intl.formatMessage(i18n.retry)}
        </Button>
      </span>
    );
  }

  // failed
  return (
    <span className="inline-flex items-center gap-1.5 text-xs text-red-700 dark:text-red-400">
      <AlertCircle className="h-3 w-3" />
      {intl.formatMessage(i18n.failed)}: {syncState.error}
      <Button variant="ghost" size="sm" className="h-5 px-2 text-xs" onClick={onRetry}>
        {intl.formatMessage(i18n.retry)}
      </Button>
    </span>
  );
}
