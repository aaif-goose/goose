import { useCallback, useEffect, useState } from 'react';
import {
  AlertCircle,
  BarChart3,
  ListChecks,
  Plug,
  Settings as SettingsIcon,
} from 'lucide-react';
import { MainPanelLayout } from '../Layout/MainPanelLayout';
import { ScrollArea } from '../ui/scroll-area';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../ui/tabs';
import { Card, CardContent } from '../ui/card';
import { Button } from '../ui/button';
import { getTunnelStatus } from '../../api/sdk.gen';
import type { TunnelInfo } from '../../api/types.gen';
import { defineMessages, useIntl } from '../../i18n';
import CopilotGeneral from './sections/CopilotGeneral';
import CopilotCodeReview from './sections/CopilotCodeReview';
import CopilotAnalytics from './sections/CopilotAnalytics';
import CopilotConnectors from './sections/CopilotConnectors';
import { SaveIndicator } from './SaveIndicator';
import { useCopilotPrefs } from './useCopilotPrefs';

const STORAGE_KEY = 'goose-copilot:installation';

export interface StoredInstall {
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

const i18n = defineMessages({
  pageTitle: {
    id: 'copilotView.pageTitle',
    defaultMessage: 'Goose Copilot',
  },
  tabGeneral: {
    id: 'copilotView.tabGeneral',
    defaultMessage: 'General',
  },
  tabCodeReview: {
    id: 'copilotView.tabCodeReview',
    defaultMessage: 'Code review',
  },
  tabAnalytics: {
    id: 'copilotView.tabAnalytics',
    defaultMessage: 'Analytics',
  },
  tabConnectors: {
    id: 'copilotView.tabConnectors',
    defaultMessage: 'Connectors',
  },
  notConnectedBannerTitle: {
    id: 'copilotView.notConnectedBannerTitle',
    defaultMessage: 'Goose Copilot is not connected yet',
  },
  notConnectedBannerBody: {
    id: 'copilotView.notConnectedBannerBody',
    defaultMessage:
      'Connect the GitHub App in the Connectors tab to start receiving automated PR reviews.',
  },
  notConnectedBannerCta: {
    id: 'copilotView.notConnectedBannerCta',
    defaultMessage: 'Go to Connectors',
  },
});

export default function CopilotView() {
  const intl = useIntl();
  const [activeTab, setActiveTab] = useState('general');
  const [tunnelInfo, setTunnelInfo] = useState<TunnelInfo>({
    state: 'idle',
    url: '',
    hostname: '',
    secret: '',
  });
  const [stored, setStored] = useState<StoredInstall | null>(loadStoredInstall);
  const { prefs, update: updatePrefs, retry: retryPrefs, syncState } = useCopilotPrefs();

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

  const handleInstallSaved = (record: StoredInstall) => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(record));
    setStored(record);
    refreshTunnel();
  };

  const handleInstallCleared = () => {
    localStorage.removeItem(STORAGE_KEY);
    setStored(null);
  };

  const isConnected = stored !== null && tunnelInfo.state === 'running';

  return (
    <MainPanelLayout>
      <div className="flex-1 flex flex-col min-h-0">
        <div className="bg-background-primary px-8 pb-8 pt-16">
          <div className="flex flex-col page-transition">
            <div className="flex justify-between items-center mb-1">
              <h1 className="text-4xl font-light">{intl.formatMessage(i18n.pageTitle)}</h1>
              <SaveIndicator syncState={syncState} onRetry={retryPrefs} />
            </div>
          </div>
        </div>

        <div className="flex-1 min-h-0 relative px-6">
          {!isConnected && activeTab !== 'connectors' && (
            <Card className="rounded-lg border-amber-300 dark:border-amber-800 mb-4 mr-4">
              <CardContent className="pt-4 px-4 flex items-start gap-3">
                <AlertCircle className="h-5 w-5 text-amber-600 mt-0.5 shrink-0" />
                <div className="flex-1">
                  <p className="text-text-primary text-xs font-medium">
                    {intl.formatMessage(i18n.notConnectedBannerTitle)}
                  </p>
                  <p className="text-xs text-text-secondary max-w-md mt-[2px]">
                    {intl.formatMessage(i18n.notConnectedBannerBody)}
                  </p>
                </div>
                <Button size="sm" variant="default" onClick={() => setActiveTab('connectors')}>
                  {intl.formatMessage(i18n.notConnectedBannerCta)}
                </Button>
              </CardContent>
            </Card>
          )}

          <Tabs value={activeTab} onValueChange={setActiveTab} className="h-full flex flex-col">
            <div className="px-1">
              <TabsList className="w-full mb-2 justify-start overflow-x-auto flex-nowrap">
                <TabsTrigger
                  value="general"
                  className="flex gap-2"
                  data-testid="copilot-general-tab"
                >
                  <SettingsIcon className="h-4 w-4" />
                  {intl.formatMessage(i18n.tabGeneral)}
                </TabsTrigger>
                <TabsTrigger
                  value="code-review"
                  className="flex gap-2"
                  data-testid="copilot-code-review-tab"
                >
                  <ListChecks className="h-4 w-4" />
                  {intl.formatMessage(i18n.tabCodeReview)}
                </TabsTrigger>
                <TabsTrigger
                  value="analytics"
                  className="flex gap-2"
                  data-testid="copilot-analytics-tab"
                >
                  <BarChart3 className="h-4 w-4" />
                  {intl.formatMessage(i18n.tabAnalytics)}
                </TabsTrigger>
                <TabsTrigger
                  value="connectors"
                  className="flex gap-2"
                  data-testid="copilot-connectors-tab"
                >
                  <Plug className="h-4 w-4" />
                  {intl.formatMessage(i18n.tabConnectors)}
                </TabsTrigger>
              </TabsList>
            </div>

            <ScrollArea className="flex-1 px-2">
              <TabsContent
                value="general"
                className="mt-0 focus-visible:outline-none focus-visible:ring-0"
              >
                <CopilotGeneral prefs={prefs} onUpdate={updatePrefs} />
              </TabsContent>

              <TabsContent
                value="code-review"
                className="mt-0 focus-visible:outline-none focus-visible:ring-0"
              >
                <CopilotCodeReview prefs={prefs} onUpdate={updatePrefs} />
              </TabsContent>

              <TabsContent
                value="analytics"
                className="mt-0 focus-visible:outline-none focus-visible:ring-0"
              >
                <CopilotAnalytics />
              </TabsContent>

              <TabsContent
                value="connectors"
                className="mt-0 focus-visible:outline-none focus-visible:ring-0"
              >
                <CopilotConnectors
                  tunnelInfo={tunnelInfo}
                  stored={stored}
                  onInstallSaved={handleInstallSaved}
                  onInstallCleared={handleInstallCleared}
                  onRefreshTunnel={refreshTunnel}
                />
              </TabsContent>
            </ScrollArea>
          </Tabs>
        </div>
      </div>
    </MainPanelLayout>
  );
}
