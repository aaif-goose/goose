import { BarChart3, Info } from 'lucide-react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../../ui/card';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  comingSoonTitle: {
    id: 'copilotAnalytics.comingSoonTitle',
    defaultMessage: 'Analytics is wiring up',
  },
  comingSoonBody: {
    id: 'copilotAnalytics.comingSoonBody',
    defaultMessage:
      'Goose Copilot does not record review metrics yet. These cards show the shape of what is coming — reviews delivered, issues found by severity, and reaction sentiment from PR authors.',
  },
  issuesRaised: {
    id: 'copilotAnalytics.issuesRaised',
    defaultMessage: 'Issues raised',
  },
  prsReviewed: {
    id: 'copilotAnalytics.prsReviewed',
    defaultMessage: 'PRs reviewed',
  },
  commitsPushed: {
    id: 'copilotAnalytics.commitsPushed',
    defaultMessage: 'Commits pushed',
  },
  issuesBySeverity: {
    id: 'copilotAnalytics.issuesBySeverity',
    defaultMessage: 'Issues found by severity',
  },
  reactionSentiment: {
    id: 'copilotAnalytics.reactionSentiment',
    defaultMessage: 'Reviewer reactions',
  },
  reactionSentimentDescription: {
    id: 'copilotAnalytics.reactionSentimentDescription',
    defaultMessage: '👍 / 👎 reactions left on Goose Copilot review comments.',
  },
  noData: {
    id: 'copilotAnalytics.noData',
    defaultMessage: 'No data for this period',
  },
});

export default function CopilotAnalytics() {
  const intl = useIntl();

  return (
    <div className="space-y-4 pr-4 pb-8 mt-1">
      <Card className="rounded-lg border-dashed">
        <CardContent className="pt-4 px-4 flex items-start gap-3">
          <Info className="h-5 w-5 text-text-secondary mt-0.5 shrink-0" />
          <div>
            <p className="text-text-primary text-xs font-medium">
              {intl.formatMessage(i18n.comingSoonTitle)}
            </p>
            <p className="text-xs text-text-secondary max-w-md mt-[2px]">
              {intl.formatMessage(i18n.comingSoonBody)}
            </p>
          </div>
        </CardContent>
      </Card>

      <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
        <StatCard label={intl.formatMessage(i18n.issuesRaised)} value="—" />
        <StatCard label={intl.formatMessage(i18n.prsReviewed)} value="—" />
        <StatCard label={intl.formatMessage(i18n.commitsPushed)} value="—" />
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <Card className="rounded-lg">
          <CardHeader className="pb-0">
            <CardTitle className="mb-1">{intl.formatMessage(i18n.issuesBySeverity)}</CardTitle>
          </CardHeader>
          <CardContent className="pt-4 px-4">
            <EmptyChart label={intl.formatMessage(i18n.noData)} />
          </CardContent>
        </Card>
        <Card className="rounded-lg">
          <CardHeader className="pb-0">
            <CardTitle className="mb-1">{intl.formatMessage(i18n.reactionSentiment)}</CardTitle>
            <CardDescription>
              {intl.formatMessage(i18n.reactionSentimentDescription)}
            </CardDescription>
          </CardHeader>
          <CardContent className="pt-4 px-4">
            <EmptyChart label={intl.formatMessage(i18n.noData)} />
          </CardContent>
        </Card>
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

function EmptyChart({ label }: { label: string }) {
  return (
    <div className="h-40 rounded-md border border-dashed border-border flex items-center justify-center text-xs text-text-secondary">
      <BarChart3 className="h-3.5 w-3.5 mr-2" />
      {label}
    </div>
  );
}
