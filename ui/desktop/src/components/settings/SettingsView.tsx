import { ScrollArea } from '../ui/scroll-area';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../ui/tabs';
import { View, ViewOptions } from '../../utils/navigationUtils';
import ModelsSection from './models/ModelsSection';
import SessionSharingSection from './sessions/SessionSharingSection';
import ExternalBackendSection from './app/ExternalBackendSection';
import AppSettingsSection from './app/AppSettingsSection';
import ConfigSettings from './config/ConfigSettings';
import PromptsSettingsSection from './PromptsSettingsSection';
import { ExtensionConfig } from '../../api';
import { MainPanelLayout } from '../Layout/MainPanelLayout';
import {
  Bot,
  Share2,
  Monitor,
  MessageSquare,
  FileText,
  Keyboard,
  HardDrive,
  Network,
} from 'lucide-react';
import { useState, useEffect, useRef } from 'react';
import TunnelSection from './tunnel/TunnelSection';
import GatewaySettingsSection from './gateways/GatewaySettingsSection';
import { getTunnelStatus } from '../../api/sdk.gen';
import ChatSettingsSection from './chat/ChatSettingsSection';
import KeyboardShortcutsSection from './keyboard/KeyboardShortcutsSection';
import LocalInferenceSection from './localInference/LocalInferenceSection';
import MeshSection from './mesh/MeshSection';
import { CONFIGURATION_ENABLED } from '../../updates';
import { trackSettingsTabViewed } from '../../utils/analytics';
import { useFeatures } from '../../contexts/FeaturesContext';
import { defineMessages, useIntl } from '../../i18n';

const i18n = defineMessages({
  title: {
    id: 'settingsView.title',
    defaultMessage: 'Settings',
  },
  tabModels: {
    id: 'settingsView.tabModels',
    defaultMessage: 'Models',
  },
  tabLocalInference: {
    id: 'settingsView.tabLocalInference',
    defaultMessage: 'Local Inference',
  },
  tabChat: {
    id: 'settingsView.tabChat',
    defaultMessage: 'Chat',
  },
  tabSession: {
    id: 'settingsView.tabSession',
    defaultMessage: 'Session',
  },
  tabPrompts: {
    id: 'settingsView.tabPrompts',
    defaultMessage: 'Prompts',
  },
  tabKeyboard: {
    id: 'settingsView.tabKeyboard',
    defaultMessage: 'Keyboard',
  },
  tabApp: {
    id: 'settingsView.tabApp',
    defaultMessage: 'App',
  },
});

export type SettingsViewOptions = {
  deepLinkConfig?: ExtensionConfig;
  showEnvVars?: boolean;
  section?: string;
  sessionId?: string;
};

export default function SettingsView({
  onClose,
  setView,
  viewOptions,
}: {
  onClose: () => void;
  setView: (view: View, viewOptions?: ViewOptions) => void;
  viewOptions: SettingsViewOptions;
}) {
  const [activeTab, setActiveTab] = useState('models');
  const [tunnelDisabled, setTunnelDisabled] = useState(false);
  const hasTrackedInitialTab = useRef(false);
  const { localInference } = useFeatures();
  const intl = useIntl();

  const handleTabChange = (tab: string) => {
    setActiveTab(tab);
    trackSettingsTabViewed(tab);
  };

  // Determine initial tab based on section prop
  useEffect(() => {
    if (viewOptions.section) {
      // Map section names to tab values
      const sectionToTab: Record<string, string> = {
        update: 'app',
        models: 'models',
        modes: 'chat',
        sharing: 'sharing',
        styles: 'chat',
        tools: 'chat',
        app: 'app',
        chat: 'chat',
        prompts: 'prompts',
        keyboard: 'keyboard',
        gateway: 'sharing',
        'local-inference': 'local-inference',
        mesh: 'mesh',
      };

      const targetTab = sectionToTab[viewOptions.section];
      if (
        targetTab &&
        (targetTab !== 'local-inference' || localInference) &&
        (targetTab !== 'mesh' || !tunnelDisabled)
      ) {
        setActiveTab(targetTab);
      }
    }
  }, [viewOptions.section, localInference, tunnelDisabled]);

  // Reset active tab if local-inference or mesh becomes unavailable
  useEffect(() => {
    if (!localInference && activeTab === 'local-inference') {
      setActiveTab('models');
    }
    if (tunnelDisabled && activeTab === 'mesh') {
      setActiveTab('models');
    }
  }, [localInference, tunnelDisabled, activeTab]);

  useEffect(() => {
    if (!hasTrackedInitialTab.current) {
      trackSettingsTabViewed(activeTab);
      hasTrackedInitialTab.current = true;
    }
  }, [activeTab]);

  useEffect(() => {
    getTunnelStatus()
      .then(({ data }) => {
        setTunnelDisabled(data?.state === 'disabled');
      })
      .catch(() => {
        setTunnelDisabled(false);
      });
  }, []);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !event.defaultPrevented) {
        onClose();
      }
    };

    document.addEventListener('keydown', handleKeyDown);

    return () => {
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, [onClose]);

  return (
    <>
      <MainPanelLayout>
        <div className="flex-1 flex flex-col min-h-0">
          <div className="bg-background-primary px-8 pb-8 pt-16">
            <div className="flex flex-col page-transition">
              <div className="flex justify-between items-center mb-1">
                <h1 className="text-4xl font-light">{intl.formatMessage(i18n.title)}</h1>
              </div>
            </div>
          </div>

          <div className="flex-1 min-h-0 relative px-6">
            <Tabs
              value={activeTab}
              onValueChange={handleTabChange}
              className="h-full flex flex-col"
            >
              <div className="px-1">
                <TabsList className="w-full mb-2 justify-start overflow-x-auto flex-nowrap">
                  <TabsTrigger
                    value="models"
                    className="flex gap-2"
                    data-testid="settings-models-tab"
                  >
                    <Bot className="h-4 w-4" />
                    {intl.formatMessage(i18n.tabModels)}
                  </TabsTrigger>
                  {localInference && (
                    <TabsTrigger
                      value="local-inference"
                      className="flex gap-2"
                      data-testid="settings-local-inference-tab"
                    >
                      <HardDrive className="h-4 w-4" />
                      {intl.formatMessage(i18n.tabLocalInference)}
                    </TabsTrigger>
                  )}
                  {!tunnelDisabled && (
                    <TabsTrigger
                      value="mesh"
                      className="flex gap-2"
                      data-testid="settings-mesh-tab"
                    >
                      <Network className="h-4 w-4" />
                      Mesh
                    </TabsTrigger>
                  )}
                  <TabsTrigger value="chat" className="flex gap-2" data-testid="settings-chat-tab">
                    <MessageSquare className="h-4 w-4" />
                    {intl.formatMessage(i18n.tabChat)}
                  </TabsTrigger>
                  <TabsTrigger
                    value="sharing"
                    className="flex gap-2"
                    data-testid="settings-sharing-tab"
                  >
                    <Share2 className="h-4 w-4" />
                    {intl.formatMessage(i18n.tabSession)}
                  </TabsTrigger>
                  <TabsTrigger
                    value="prompts"
                    className="flex gap-2"
                    data-testid="settings-prompts-tab"
                  >
                    <FileText className="h-4 w-4" />
                    {intl.formatMessage(i18n.tabPrompts)}
                  </TabsTrigger>
                  <TabsTrigger
                    value="keyboard"
                    className="flex gap-2"
                    data-testid="settings-keyboard-tab"
                  >
                    <Keyboard className="h-4 w-4" />
                    {intl.formatMessage(i18n.tabKeyboard)}
                  </TabsTrigger>
                  <TabsTrigger value="app" className="flex gap-2" data-testid="settings-app-tab">
                    <Monitor className="h-4 w-4" />
                    {intl.formatMessage(i18n.tabApp)}
                  </TabsTrigger>
                </TabsList>
              </div>

              <ScrollArea className="flex-1 px-2">
                <TabsContent
                  value="models"
                  className="mt-0 focus-visible:outline-none focus-visible:ring-0"
                >
                  <ModelsSection setView={setView} />
                </TabsContent>

                {localInference && (
                  <TabsContent
                    value="local-inference"
                    className="mt-0 focus-visible:outline-none focus-visible:ring-0"
                  >
                    <LocalInferenceSection />
                  </TabsContent>
                )}

                {!tunnelDisabled && (
                  <TabsContent
                    value="mesh"
                    className="mt-0 focus-visible:outline-none focus-visible:ring-0"
                  >
                    <MeshSection />
                  </TabsContent>
                )}

                <TabsContent
                  value="chat"
                  className="mt-0 focus-visible:outline-none focus-visible:ring-0"
                >
                  <ChatSettingsSection sessionId={viewOptions.sessionId} />
                </TabsContent>

                <TabsContent
                  value="sharing"
                  className="mt-0 focus-visible:outline-none focus-visible:ring-0"
                >
                  <div className="space-y-8 pb-8">
                    <SessionSharingSection />
                    <ExternalBackendSection />
                    {!tunnelDisabled && (
                      <div className="space-y-4">
                        <TunnelSection />
                        <GatewaySettingsSection />
                      </div>
                    )}
                  </div>
                </TabsContent>

                <TabsContent
                  value="prompts"
                  className="mt-0 focus-visible:outline-none focus-visible:ring-0"
                >
                  <PromptsSettingsSection />
                </TabsContent>

                <TabsContent
                  value="keyboard"
                  className="mt-0 focus-visible:outline-none focus-visible:ring-0"
                >
                  <KeyboardShortcutsSection />
                </TabsContent>

                <TabsContent
                  value="app"
                  className="mt-0 focus-visible:outline-none focus-visible:ring-0"
                >
                  <div className="space-y-8">
                    {CONFIGURATION_ENABLED && <ConfigSettings />}
                    <AppSettingsSection scrollToSection={viewOptions.section} />
                  </div>
                </TabsContent>
              </ScrollArea>
            </Tabs>
          </div>
        </div>
      </MainPanelLayout>
    </>
  );
}
