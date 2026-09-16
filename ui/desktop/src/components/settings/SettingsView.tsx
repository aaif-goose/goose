import { ScrollArea } from '../ui/scroll-area';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../ui/tabs';
import { View, ViewOptions } from '../../utils/navigationUtils';
import ModelsSection from './models/ModelsSection';
import ExternalBackendSection from './app/ExternalBackendSection';
import AgentLoopSettings from './AgentLoopSettings';
import AppSettingsSection from './app/AppSettingsSection';
import ConfigSettings from './config/ConfigSettings';
import PromptsSettingsSection from './PromptsSettingsSection';
import type { ExtensionConfig } from '../../types/extensions';
import {
  Bot,
  Share2,
  Monitor,
  MessageSquare,
  FileText,
  Keyboard,
  HardDrive,
  KeyRound,
} from 'lucide-react';
import { useState, useEffect, useRef } from 'react';
import ChatSettingsSection from './chat/ChatSettingsSection';
import KeyboardShortcutsSection from './keyboard/KeyboardShortcutsSection';
import AuthSettingsSection from './auth/AuthSettingsSection';
import LocalInferenceSection from './localInference/LocalInferenceSection';
import { CONFIGURATION_ENABLED } from '../../updates';
import { trackSettingsTabViewed } from '../../utils/analytics';
import { useFeatures } from '../../contexts/FeaturesContext';
import { defineMessages, useIntl } from '../../i18n';
import BackButton from '../ui/BackButton';
import { useNavigationContext } from '../Layout/NavigationContext';

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
  tabAgent: {
    id: 'settingsView.tabExternalBackend',
    defaultMessage: 'Agent',
  },
  tabPrompts: {
    id: 'settingsView.tabPrompts',
    defaultMessage: 'Prompts',
  },
  tabKeyboard: {
    id: 'settingsView.tabKeyboard',
    defaultMessage: 'Keyboard',
  },
  tabAuth: {
    id: 'settingsView.tabAuth',
    defaultMessage: 'Auth',
  },
  tabApp: {
    id: 'settingsView.tabApp',
    defaultMessage: 'App',
  },
});

const settingsTabClass =
  'w-full gap-3 rounded-full px-3 py-2 text-sm font-medium hover:bg-background-tertiary/60 data-[state=active]:bg-background-tertiary data-[state=active]:shadow-none';

export type SettingsViewOptions = {
  deepLinkConfig?: ExtensionConfig;
  showEnvVars?: boolean;
  section?: string;
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
  const hasTrackedInitialTab = useRef(false);
  const { localInference } = useFeatures();
  const { navWidth } = useNavigationContext();
  const intl = useIntl();

  const activeTabTitle = {
    models: intl.formatMessage(i18n.tabModels),
    'local-inference': intl.formatMessage(i18n.tabLocalInference),
    chat: intl.formatMessage(i18n.tabChat),
    sharing: intl.formatMessage(i18n.tabAgent),
    prompts: intl.formatMessage(i18n.tabPrompts),
    keyboard: intl.formatMessage(i18n.tabKeyboard),
    auth: intl.formatMessage(i18n.tabAuth),
    app: intl.formatMessage(i18n.tabApp),
  }[activeTab];

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
        agent: 'sharing',
        app: 'app',
        chat: 'chat',
        prompts: 'prompts',
        keyboard: 'keyboard',
        auth: 'auth',
        'local-inference': 'local-inference',
      };

      const targetTab = sectionToTab[viewOptions.section];
      if (targetTab && (targetTab !== 'local-inference' || localInference)) {
        setActiveTab(targetTab);
      }
    }
  }, [viewOptions.section, localInference]);

  // Reset active tab if local-inference becomes unavailable
  useEffect(() => {
    if (!localInference && activeTab === 'local-inference') {
      setActiveTab('models');
    }
  }, [localInference, activeTab]);

  useEffect(() => {
    if (!hasTrackedInitialTab.current) {
      trackSettingsTabViewed(activeTab);
      hasTrackedInitialTab.current = true;
    }
  }, [activeTab]);

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
    <div className="absolute inset-0 animate-fade-in bg-background-primary">
      <Tabs
        value={activeTab}
        onValueChange={handleTabChange}
        orientation="vertical"
        className="h-full min-h-0 w-full flex-row rounded-none"
      >
        <div className="h-full flex-shrink-0 p-2" style={{ width: navWidth }}>
          <aside
            aria-label={intl.formatMessage(i18n.title)}
            className="flex h-full w-full flex-col overflow-hidden rounded-xl border border-border-primary bg-background-primary"
          >
            <div className="h-[48px] flex-shrink-0 no-drag" />
            <div className="px-2">
              <BackButton
                onClick={onClose}
                variant="ghost"
                className="mb-3 w-full justify-start rounded-full px-3 text-sm font-medium hover:bg-background-tertiary/60"
              />
            </div>
            <TabsList className="w-full flex-col items-stretch gap-0.5 bg-transparent px-2 py-0">
              <TabsTrigger
                value="models"
                className={settingsTabClass}
                data-testid="settings-models-tab"
              >
                <Bot className="h-5 w-5 text-text-secondary" />
                {intl.formatMessage(i18n.tabModels)}
              </TabsTrigger>
              {localInference && (
                <TabsTrigger
                  value="local-inference"
                  className={settingsTabClass}
                  data-testid="settings-local-inference-tab"
                >
                  <HardDrive className="h-5 w-5 text-text-secondary" />
                  {intl.formatMessage(i18n.tabLocalInference)}
                </TabsTrigger>
              )}
              <TabsTrigger
                value="chat"
                className={settingsTabClass}
                data-testid="settings-chat-tab"
              >
                <MessageSquare className="h-5 w-5 text-text-secondary" />
                {intl.formatMessage(i18n.tabChat)}
              </TabsTrigger>
              <TabsTrigger
                value="sharing"
                className={settingsTabClass}
                data-testid="settings-sharing-tab"
              >
                <Share2 className="h-5 w-5 text-text-secondary" />
                {intl.formatMessage(i18n.tabAgent)}
              </TabsTrigger>
              <TabsTrigger
                value="prompts"
                className={settingsTabClass}
                data-testid="settings-prompts-tab"
              >
                <FileText className="h-5 w-5 text-text-secondary" />
                {intl.formatMessage(i18n.tabPrompts)}
              </TabsTrigger>
              <TabsTrigger
                value="keyboard"
                className={settingsTabClass}
                data-testid="settings-keyboard-tab"
              >
                <Keyboard className="h-5 w-5 text-text-secondary" />
                {intl.formatMessage(i18n.tabKeyboard)}
              </TabsTrigger>
              <TabsTrigger
                value="auth"
                className={settingsTabClass}
                data-testid="settings-auth-tab"
              >
                <KeyRound className="h-5 w-5 text-text-secondary" />
                {intl.formatMessage(i18n.tabAuth)}
              </TabsTrigger>
              <TabsTrigger
                value="app"
                className={settingsTabClass}
                data-testid="settings-app-tab"
              >
                <Monitor className="h-5 w-5 text-text-secondary" />
                {intl.formatMessage(i18n.tabApp)}
              </TabsTrigger>
            </TabsList>
          </aside>
        </div>

        <main className="flex min-w-0 flex-1 flex-col overflow-hidden bg-background-primary">
          <div className="px-12 pb-8 pt-16">
            <div className="mx-auto max-w-5xl">
              <h1 className="text-4xl font-light">{activeTabTitle}</h1>
            </div>
          </div>

          <ScrollArea className="min-h-0 flex-1 px-12">
            <div className="mx-auto max-w-5xl pb-10">
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

              <TabsContent
                value="chat"
                className="mt-0 focus-visible:outline-none focus-visible:ring-0"
              >
                <ChatSettingsSection />
              </TabsContent>

              <TabsContent
                value="sharing"
                className="mt-0 focus-visible:outline-none focus-visible:ring-0"
              >
                <div className="space-y-4 pb-8">
                  <AgentLoopSettings />
                  <ExternalBackendSection />
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
                value="auth"
                className="mt-0 focus-visible:outline-none focus-visible:ring-0"
              >
                <AuthSettingsSection />
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
            </div>
          </ScrollArea>
        </main>
      </Tabs>
    </div>
  );
}
