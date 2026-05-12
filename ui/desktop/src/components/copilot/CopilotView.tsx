import { useCallback, useEffect, useState } from 'react';
import { AlertCircle, Bot, CheckCircle2, ExternalLink, Loader2 } from 'lucide-react';
import { MainPanelLayout } from '../Layout/MainPanelLayout';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../ui/card';
import { Button } from '../ui/button';
import { getTunnelStatus } from '../../api/sdk.gen';
import type { TunnelInfo } from '../../api/types.gen';
import { errorMessage } from '../../utils/conversionUtils';
import { defineMessages, useIntl } from '../../i18n';

const STORAGE_KEY = 'goose-copilot:installation';

const i18n = defineMessages({
  pageTitle: {
    id: 'copilotView.pageTitle',
    defaultMessage: 'Goose Copilot',
  },
  pageDescription: {
    id: 'copilotView.pageDescription',
    defaultMessage:
      'Auto-review pull requests using your local goose. Your code and API keys never leave your machine.',
  },
  cardTitle: {
    id: 'copilotView.cardTitle',
    defaultMessage: 'Code review on pull requests',
  },
  cardDescription: {
    id: 'copilotView.cardDescription',
    defaultMessage:
      'When enabled, every PR opened on a connected repository triggers a review on your local goose. The review is posted as inline comments on the PR.',
  },
  statusLocalTunnel: {
    id: 'copilotView.statusLocalTunnel',
    defaultMessage: 'Local tunnel',
  },
  statusGithubApp: {
    id: 'copilotView.statusGithubApp',
    defaultMessage: 'GitHub App',
  },
  tunnelRunning: {
    id: 'copilotView.tunnelRunning',
    defaultMessage: 'Running',
  },
  tunnelStarting: {
    id: 'copilotView.tunnelStarting',
    defaultMessage: 'Starting…',
  },
  tunnelError: {
    id: 'copilotView.tunnelError',
    defaultMessage: 'Error',
  },
  tunnelIdle: {
    id: 'copilotView.tunnelIdle',
    defaultMessage: 'Idle',
  },
  installationLabel: {
    id: 'copilotView.installationLabel',
    defaultMessage: 'Installation {id}',
  },
  notConnected: {
    id: 'copilotView.notConnected',
    defaultMessage: 'Not connected',
  },
  awaitingInstall: {
    id: 'copilotView.awaitingInstall',
    defaultMessage: 'Finish installing on GitHub in your browser…',
  },
  awaitingInstallHint: {
    id: 'copilotView.awaitingInstallHint',
    defaultMessage:
      'A browser tab just opened to GitHub. Pick the repos you want reviewed and click Install & Authorize. This page will pick it up automatically.',
  },
  installAction: {
    id: 'copilotView.installAction',
    defaultMessage: 'Connect GitHub',
  },
  activeMessage: {
    id: 'copilotView.activeMessage',
    defaultMessage: 'Goose Copilot is active. Open a PR to try it.',
  },
  setupFailed: {
    id: 'copilotView.setupFailed',
    defaultMessage: 'Failed to connect Goose Copilot',
  },
  disconnectAction: {
    id: 'copilotView.disconnectAction',
    defaultMessage: 'Disconnect',
  },
  disconnectHint: {
    id: 'copilotView.disconnectHint',
    defaultMessage:
      'Opens the GitHub App settings so you can uninstall. After you uninstall on GitHub, this side disconnects automatically.',
  },
});

interface StoredInstall {
  installationId: number;
  enabledAt: string;
}

function loadStoredInstall(): StoredInstall | null {
  const raw = localStorage.getItem(STORAGE_KEY);
  if (!raw) return null;
  try {
    return JSON.parse(raw) as StoredInstall;
  } catch {
    return null;
  }
}

export default function CopilotView() {
  const intl = useIntl();
  const [tunnelInfo, setTunnelInfo] = useState<TunnelInfo>({
    state: 'idle',
    url: '',
    hostname: '',
    secret: '',
  });
  const [stored, setStored] = useState<StoredInstall | null>(loadStoredInstall);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refreshTunnel = useCallback(async () => {
    try {
      const { data } = await getTunnelStatus();
      if (data) setTunnelInfo(data);
    } catch (err) {
      console.error('Failed to read tunnel status:', err);
    }
  }, []);

  useEffect(() => {
    refreshTunnel();
  }, [refreshTunnel]);

  const isEnabled = stored !== null && tunnelInfo.state === 'running';

  const handleDisconnect = () => {
    if (!stored) return;
    const url = `https://github.com/settings/installations/${stored.installationId}`;
    window.electron.openExternal(url);
    localStorage.removeItem(STORAGE_KEY);
    setStored(null);
  };

  const handleConnect = async () => {
    setError(null);
    setBusy(true);
    try {
      const host = await window.electron.getGoosedHostPort();
      const secret = await window.electron.getSecretKey();
      if (!host) throw new Error('goosed is not running');

      const res = await fetch(`${host}/copilot/setup`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'X-Secret-Key': secret,
        },
      });
      if (!res.ok) {
        const detail = await res.text();
        throw new Error(`${res.status}: ${detail}`);
      }
      const body = (await res.json()) as { installation_id: number };
      const record: StoredInstall = {
        installationId: body.installation_id,
        enabledAt: new Date().toISOString(),
      };
      localStorage.setItem(STORAGE_KEY, JSON.stringify(record));
      setStored(record);
      await refreshTunnel();
    } catch (err) {
      setError(errorMessage(err, intl.formatMessage(i18n.setupFailed)));
    } finally {
      setBusy(false);
    }
  };

  const tunnelStatusLabel = (): string => {
    switch (tunnelInfo.state) {
      case 'running':
        return intl.formatMessage(i18n.tunnelRunning);
      case 'starting':
        return intl.formatMessage(i18n.tunnelStarting);
      case 'error':
        return intl.formatMessage(i18n.tunnelError);
      default:
        return intl.formatMessage(i18n.tunnelIdle);
    }
  };

  return (
    <MainPanelLayout>
      <div className="flex-1 overflow-auto">
        <div className="max-w-2xl mx-auto p-6 space-y-4">
          <div className="flex items-center gap-3">
            <Bot className="h-7 w-7" />
            <div>
              <h1 className="text-xl font-semibold">{intl.formatMessage(i18n.pageTitle)}</h1>
              <p className="text-sm text-text-secondary">
                {intl.formatMessage(i18n.pageDescription)}
              </p>
            </div>
          </div>

          <Card>
            <CardHeader>
              <CardTitle className="text-base">{intl.formatMessage(i18n.cardTitle)}</CardTitle>
              <CardDescription>{intl.formatMessage(i18n.cardDescription)}</CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <StatusRow
                label={intl.formatMessage(i18n.statusLocalTunnel)}
                ok={tunnelInfo.state === 'running'}
                detail={tunnelStatusLabel()}
              />
              <StatusRow
                label={intl.formatMessage(i18n.statusGithubApp)}
                ok={stored !== null}
                detail={
                  stored
                    ? intl.formatMessage(i18n.installationLabel, { id: stored.installationId })
                    : intl.formatMessage(i18n.notConnected)
                }
              />

              {error && (
                <div className="p-3 bg-red-100 dark:bg-red-900/20 border border-red-300 dark:border-red-800 rounded text-sm text-red-800 dark:text-red-200">
                  {error}
                </div>
              )}

              {isEnabled ? (
                <div className="pt-2 space-y-2">
                  <div className="flex items-center justify-between">
                    <p className="text-sm text-green-700 dark:text-green-400">
                      {intl.formatMessage(i18n.activeMessage)}
                    </p>
                    <Button variant="outline" size="sm" onClick={handleDisconnect}>
                      {intl.formatMessage(i18n.disconnectAction)}
                    </Button>
                  </div>
                  <p className="text-xs text-text-secondary">
                    {intl.formatMessage(i18n.disconnectHint)}
                  </p>
                </div>
              ) : busy ? (
                <div className="space-y-2 pt-2">
                  <div className="flex items-center gap-2 text-sm text-text-secondary">
                    <Loader2 className="h-4 w-4 animate-spin" />
                    {intl.formatMessage(i18n.awaitingInstall)}
                  </div>
                  <p className="text-xs text-text-secondary">
                    {intl.formatMessage(i18n.awaitingInstallHint)}
                  </p>
                </div>
              ) : (
                <div className="pt-2">
                  <Button variant="default" onClick={handleConnect}>
                    <ExternalLink className="h-4 w-4 mr-2" />
                    {intl.formatMessage(i18n.installAction)}
                  </Button>
                </div>
              )}
            </CardContent>
          </Card>
        </div>
      </div>
    </MainPanelLayout>
  );
}

function StatusRow({ label, ok, detail }: { label: string; ok: boolean; detail: string }) {
  return (
    <div className="flex items-center justify-between text-sm">
      <span className="text-text-secondary">{label}</span>
      <span className="flex items-center gap-2">
        {ok ? (
          <CheckCircle2 className="h-4 w-4 text-green-600" />
        ) : (
          <AlertCircle className="h-4 w-4 text-text-secondary" />
        )}
        <span>{detail}</span>
      </span>
    </div>
  );
}
