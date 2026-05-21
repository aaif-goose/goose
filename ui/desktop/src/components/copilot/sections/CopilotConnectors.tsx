import { useState } from 'react';
import {
  AlertCircle,
  CheckCircle2,
  ExternalLink,
  Github,
  Loader2,
  MessageSquare,
  Plug,
  Workflow,
} from 'lucide-react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../../ui/card';
import { Button } from '../../ui/button';
import type { TunnelInfo } from '../../../api/types.gen';
import { disconnectCopilot, startCopilotSetup } from '../../../utils/copilotSetup';
import { errorMessage } from '../../../utils/conversionUtils';
import { defineMessages, useIntl } from '../../../i18n';
import type { StoredInstall } from '../CopilotView';

const i18n = defineMessages({
  githubTitle: {
    id: 'copilotConnectors.githubTitle',
    defaultMessage: 'GitHub',
  },
  githubDescription: {
    id: 'copilotConnectors.githubDescription',
    defaultMessage: 'Install the Goose Copilot GitHub App on the repos you want auto-reviewed.',
  },
  installAction: {
    id: 'copilotConnectors.installAction',
    defaultMessage: 'Connect GitHub',
  },
  disconnectAction: {
    id: 'copilotConnectors.disconnectAction',
    defaultMessage: 'Disconnect',
  },
  manageAction: {
    id: 'copilotConnectors.manageAction',
    defaultMessage: 'Manage on GitHub',
  },
  connectedLabel: {
    id: 'copilotConnectors.connectedLabel',
    defaultMessage: 'Connected — installation {id}',
  },
  notConnected: {
    id: 'copilotConnectors.notConnected',
    defaultMessage: 'Not connected',
  },
  awaitingInstall: {
    id: 'copilotConnectors.awaitingInstall',
    defaultMessage: 'Finish installing on GitHub in your browser…',
  },
  awaitingInstallHint: {
    id: 'copilotConnectors.awaitingInstallHint',
    defaultMessage:
      'A browser tab just opened to GitHub. Pick the repos you want reviewed and click Install & Authorize.',
  },
  setupFailed: {
    id: 'copilotConnectors.setupFailed',
    defaultMessage: 'Failed to connect Goose Copilot',
  },
  disconnectHint: {
    id: 'copilotConnectors.disconnectHint',
    defaultMessage:
      'Opens the GitHub App settings so you can uninstall. After you uninstall on GitHub, this side disconnects automatically.',
  },
  slackTitle: {
    id: 'copilotConnectors.slackTitle',
    defaultMessage: 'Slack',
  },
  slackDescription: {
    id: 'copilotConnectors.slackDescription',
    defaultMessage: 'Ask @goose-copilot questions or trigger reviews from a Slack channel.',
  },
  linearTitle: {
    id: 'copilotConnectors.linearTitle',
    defaultMessage: 'Linear',
  },
  linearDescription: {
    id: 'copilotConnectors.linearDescription',
    defaultMessage: 'Mention @goose-copilot on issues and let it open PRs to address them.',
  },
  comingSoon: {
    id: 'copilotConnectors.comingSoon',
    defaultMessage: 'Coming soon',
  },
});

interface Props {
  tunnelInfo: TunnelInfo;
  stored: StoredInstall | null;
  onInstallSaved: (record: StoredInstall) => void;
  onInstallCleared: () => void;
  onRefreshTunnel: () => Promise<void>;
}

export default function CopilotConnectors({
  tunnelInfo,
  stored,
  onInstallSaved,
  onInstallCleared,
  onRefreshTunnel,
}: Props) {
  const intl = useIntl();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const isConnected = stored !== null && tunnelInfo.state === 'running';

  const handleConnect = async () => {
    setError(null);
    setBusy(true);
    try {
      const body = await startCopilotSetup();
      onInstallSaved({
        installationId: body.installation_id,
        enabledAt: new Date().toISOString(),
      });
      await onRefreshTunnel();
    } catch (err) {
      setError(errorMessage(err, intl.formatMessage(i18n.setupFailed)));
    } finally {
      setBusy(false);
    }
  };

  const handleDisconnect = async () => {
    if (!stored) return;
    setError(null);
    setBusy(true);
    try {
      await disconnectCopilot();
      onInstallCleared();
      window.electron.openExternal(
        `https://github.com/settings/installations/${stored.installationId}`
      );
    } catch (err) {
      setError(errorMessage(err, intl.formatMessage(i18n.setupFailed)));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="space-y-4 pr-4 pb-8 mt-1">
      <Card className="rounded-lg">
        <CardHeader className="pb-0">
          <div className="flex items-start gap-3">
            <Github className="h-5 w-5 mt-0.5 shrink-0" />
            <div className="flex-1">
              <CardTitle className="mb-1">{intl.formatMessage(i18n.githubTitle)}</CardTitle>
              <CardDescription>{intl.formatMessage(i18n.githubDescription)}</CardDescription>
            </div>
          </div>
        </CardHeader>
        <CardContent className="pt-4 px-4 space-y-4">
          <div className="flex items-center justify-between">
            <h3 className="text-text-primary text-xs">
              {isConnected
                ? intl.formatMessage(i18n.connectedLabel, { id: stored?.installationId ?? '' })
                : intl.formatMessage(i18n.notConnected)}
            </h3>
            {isConnected ? (
              <CheckCircle2 className="h-3.5 w-3.5 text-green-600" />
            ) : (
              <AlertCircle className="h-3.5 w-3.5 text-text-secondary" />
            )}
          </div>

          {error && (
            <div className="p-3 bg-red-100 dark:bg-red-900/20 border border-red-300 dark:border-red-800 rounded text-xs text-red-800 dark:text-red-200">
              {error}
            </div>
          )}

          {isConnected ? (
            <div className="space-y-2">
              <div className="flex gap-2">
                <Button variant="outline" size="sm" onClick={handleDisconnect}>
                  {intl.formatMessage(i18n.disconnectAction)}
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() =>
                    window.electron.openExternal(
                      `https://github.com/settings/installations/${stored?.installationId}`
                    )
                  }
                >
                  <ExternalLink className="h-3.5 w-3.5 mr-2" />
                  {intl.formatMessage(i18n.manageAction)}
                </Button>
              </div>
              <p className="text-xs text-text-secondary max-w-md mt-[2px]">
                {intl.formatMessage(i18n.disconnectHint)}
              </p>
            </div>
          ) : busy ? (
            <div className="space-y-2">
              <div className="flex items-center gap-2 text-xs text-text-secondary">
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
                {intl.formatMessage(i18n.awaitingInstall)}
              </div>
              <p className="text-xs text-text-secondary max-w-md mt-[2px]">
                {intl.formatMessage(i18n.awaitingInstallHint)}
              </p>
            </div>
          ) : (
            <Button variant="default" size="sm" onClick={handleConnect}>
              <ExternalLink className="h-3.5 w-3.5 mr-2" />
              {intl.formatMessage(i18n.installAction)}
            </Button>
          )}
        </CardContent>
      </Card>

      <ConnectorPlaceholder
        icon={<MessageSquare className="h-5 w-5 mt-0.5 shrink-0" />}
        title={intl.formatMessage(i18n.slackTitle)}
        description={intl.formatMessage(i18n.slackDescription)}
        comingSoon={intl.formatMessage(i18n.comingSoon)}
      />
      <ConnectorPlaceholder
        icon={<Workflow className="h-5 w-5 mt-0.5 shrink-0" />}
        title={intl.formatMessage(i18n.linearTitle)}
        description={intl.formatMessage(i18n.linearDescription)}
        comingSoon={intl.formatMessage(i18n.comingSoon)}
      />
    </div>
  );
}

function ConnectorPlaceholder({
  title,
  description,
  comingSoon,
  icon,
}: {
  title: string;
  description: string;
  comingSoon: string;
  icon?: React.ReactNode;
}) {
  return (
    <Card className="rounded-lg opacity-70">
      <CardHeader className="pb-0">
        <div className="flex items-start gap-3">
          {icon ?? <Plug className="h-5 w-5 mt-0.5 shrink-0" />}
          <div className="flex-1">
            <CardTitle className="mb-1 flex items-center gap-2">
              {title}
              <span className="text-xs font-normal px-2 py-0.5 rounded-full bg-background-secondary text-text-secondary">
                {comingSoon}
              </span>
            </CardTitle>
            <CardDescription>{description}</CardDescription>
          </div>
        </div>
      </CardHeader>
    </Card>
  );
}
