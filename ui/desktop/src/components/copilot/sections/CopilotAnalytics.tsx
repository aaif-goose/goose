import { AlertCircle, Loader2, RefreshCw } from 'lucide-react';
import { Card, CardContent } from '../../ui/card';
import { Button } from '../../ui/button';
import { useCopilotAnalytics } from '../useCopilotAnalytics';
import { defineMessages, useIntl } from '../../../i18n';

interface Props {
  enabled: boolean;
}

const i18n = defineMessages({
  prsReviewed: {
    id: 'copilotAnalytics.prsReviewed',
    defaultMessage: 'PRs reviewed',
  },
  issuesHandled: {
    id: 'copilotAnalytics.issuesHandled',
    defaultMessage: 'Issues handled',
  },
  commitsPushed: {
    id: 'copilotAnalytics.commitsPushed',
    defaultMessage: 'Commits pushed',
  },
  loadFailed: {
    id: 'copilotAnalytics.loadFailed',
    defaultMessage: 'Could not load analytics',
  },
  retry: {
    id: 'copilotAnalytics.retry',
    defaultMessage: 'Retry',
  },
});

export default function CopilotAnalyticsSection({ enabled }: Props) {
  const intl = useIntl();
  const { state, refresh } = useCopilotAnalytics(enabled);

  if (state.kind === 'idle' || state.kind === 'loading') {
    return (
      <div className="space-y-4 pr-4 pb-8 mt-1">
        <div className="h-32 rounded-md border border-dashed border-border flex items-center justify-center text-xs text-text-secondary gap-2">
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
          Loading…
        </div>
      </div>
    );
  }

  if (state.kind === 'error') {
    return (
      <div className="space-y-4 pr-4 pb-8 mt-1">
        <div className="h-32 rounded-md border border-dashed border-border flex flex-col items-center justify-center text-xs text-text-secondary gap-2 px-6 text-center">
          <span className="flex items-center gap-2">
            <AlertCircle className="h-3.5 w-3.5 text-amber-600" />
            {intl.formatMessage(i18n.loadFailed)}: {state.error}
          </span>
          <Button size="sm" variant="outline" onClick={refresh} className="h-7 px-3 text-xs">
            <RefreshCw className="h-3 w-3 mr-1.5" />
            {intl.formatMessage(i18n.retry)}
          </Button>
        </div>
      </div>
    );
  }

  const data = state.data;
  return (
    <div className="space-y-4 pr-4 pb-8 mt-1">
      <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
        <StatCard
          label={intl.formatMessage(i18n.prsReviewed)}
          value={String(data.prs_reviewed ?? 0)}
        />
        <StatCard
          label={intl.formatMessage(i18n.issuesHandled)}
          value={String(data.issues_handled ?? 0)}
        />
        <StatCard
          label={intl.formatMessage(i18n.commitsPushed)}
          value={String(data.commits_pushed ?? 0)}
        />
      </div>
    </div>
  );
}

function StatCard({ label, value }: { label: string; value: string }) {
  return (
    <Card className="rounded-lg">
      <CardContent className="pt-4 px-4">
        <p className="text-xs uppercase tracking-wide text-text-secondary">{label}</p>
        <p className="text-3xl font-light mt-1">{value}</p>
      </CardContent>
    </Card>
  );
}
