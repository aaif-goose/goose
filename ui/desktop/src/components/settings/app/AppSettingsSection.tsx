import { useState, useEffect, useRef } from 'react';
import { defineMessages, useIntl } from '../../../i18n';
import { Switch } from '../../ui/switch';
import { Button } from '../../ui/button';
import { Settings, ChevronDown, ChevronUp } from 'lucide-react';
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '../../ui/dialog';
import UpdateSection from './UpdateSection';

import { COST_TRACKING_ENABLED, UPDATES_ENABLED } from '../../../updates';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../../ui/card';
import ThemeSelector from '../../GooseSidebar/ThemeSelector';
import BlockLogoBlack from './icons/block-lockup_black.png';
import BlockLogoWhite from './icons/block-lockup_white.png';
import TelemetrySettings from './TelemetrySettings';
import { trackSettingToggled } from '../../../utils/analytics';
import { NavigationModeSelector } from './NavigationModeSelector';
import { NavigationStyleSelector } from './NavigationStyleSelector';
import { NavigationPositionSelector } from './NavigationPositionSelector';
import { NavigationCustomizationSettings } from './NavigationCustomizationSettings';
import { NavigationProvider, useNavigationContextSafe } from '../../Layout/NavigationContext';

const i18n = defineMessages({
  appearanceTitle: { id: 'settings.appearance.title', defaultMessage: 'Appearance' },
  appearanceDesc: {
    id: 'settings.appearance.description',
    defaultMessage: 'Configure how goose appears on your system',
  },
  notifications: { id: 'settings.notifications.title', defaultMessage: 'Notifications' },
  notificationsDesc: {
    id: 'settings.notifications.description',
    defaultMessage: 'Notifications are managed by your OS - {link}',
  },
  configGuide: { id: 'settings.notifications.configGuide', defaultMessage: 'Configuration guide' },
  openSettings: { id: 'settings.notifications.openSettings', defaultMessage: 'Open Settings' },
  menuBarIcon: { id: 'settings.menuBarIcon.title', defaultMessage: 'Menu bar icon' },
  menuBarIconDesc: {
    id: 'settings.menuBarIcon.description',
    defaultMessage: 'Show goose in the menu bar',
  },
  dockIcon: { id: 'settings.dockIcon.title', defaultMessage: 'Dock icon' },
  dockIconDesc: { id: 'settings.dockIcon.description', defaultMessage: 'Show goose in the dock' },
  preventSleep: { id: 'settings.preventSleep.title', defaultMessage: 'Prevent Sleep' },
  preventSleepDesc: {
    id: 'settings.preventSleep.description',
    defaultMessage:
      'Keep your computer awake while goose is running a task (screen can still lock)',
  },
  costTracking: { id: 'settings.costTracking.title', defaultMessage: 'Cost Tracking' },
  costTrackingDesc: {
    id: 'settings.costTracking.description',
    defaultMessage: 'Show model pricing and usage costs',
  },
  themeTitle: { id: 'settings.theme.title', defaultMessage: 'Theme' },
  themeDesc: {
    id: 'settings.theme.description',
    defaultMessage: 'Customize the look and feel of goose',
  },
  navigationTitle: { id: 'settings.navigation.title', defaultMessage: 'Navigation' },
  navigationDesc: {
    id: 'settings.navigation.description',
    defaultMessage: 'Customize navigation layout and behavior',
  },
  navMode: { id: 'settings.navigation.mode', defaultMessage: 'Mode' },
  navStyle: { id: 'settings.navigation.style', defaultMessage: 'Style' },
  navPosition: { id: 'settings.navigation.position', defaultMessage: 'Position' },
  navCustomize: { id: 'settings.navigation.customize', defaultMessage: 'Customize Items' },
  helpTitle: { id: 'settings.help.title', defaultMessage: 'Help & feedback' },
  helpDesc: {
    id: 'settings.help.description',
    defaultMessage: 'Help us improve goose by reporting issues or requesting new features',
  },
  reportBug: { id: 'settings.help.reportBug', defaultMessage: 'Report a Bug' },
  requestFeature: { id: 'settings.help.requestFeature', defaultMessage: 'Request a Feature' },
  versionTitle: { id: 'settings.version.title', defaultMessage: 'Version' },
  updatesTitle: { id: 'settings.updates.title', defaultMessage: 'Updates' },
  updatesDesc: {
    id: 'settings.updates.description',
    defaultMessage: 'Check for and install updates to keep goose running at its best',
  },
  notificationsModalTitle: {
    id: 'settings.notifications.modal.title',
    defaultMessage: 'How to Enable Notifications',
  },
  notificationsMacInstructions: {
    id: 'settings.notifications.modal.macInstructions',
    defaultMessage: 'To enable notifications on macOS:',
  },
  notificationsMacStep1: {
    id: 'settings.notifications.modal.macStep1',
    defaultMessage: 'Open System Preferences',
  },
  notificationsMacStep2: {
    id: 'settings.notifications.modal.macStep2',
    defaultMessage: 'Click on Notifications',
  },
  notificationsMacStep3: {
    id: 'settings.notifications.modal.macStep3',
    defaultMessage: 'Find and select goose in the application list',
  },
  notificationsMacStep4: {
    id: 'settings.notifications.modal.macStep4',
    defaultMessage: 'Enable notifications and adjust settings as desired',
  },
  notificationsWinInstructions: {
    id: 'settings.notifications.modal.winInstructions',
    defaultMessage: 'To enable notifications on Windows:',
  },
  notificationsWinStep1: {
    id: 'settings.notifications.modal.winStep1',
    defaultMessage: 'Open Settings',
  },
  notificationsWinStep2: {
    id: 'settings.notifications.modal.winStep2',
    defaultMessage: 'Go to System > Notifications',
  },
  notificationsWinStep3: {
    id: 'settings.notifications.modal.winStep3',
    defaultMessage: 'Find and select goose in the application list',
  },
  notificationsWinStep4: {
    id: 'settings.notifications.modal.winStep4',
    defaultMessage: 'Toggle notifications on and adjust settings as desired',
  },
  close: { id: 'settings.close', defaultMessage: 'Close' },
});

interface AppSettingsSectionProps {
  scrollToSection?: string;
}

const NavigationSettingsContent: React.FC = () => {
  const [isExpanded, setIsExpanded] = useState(false);
  const navContext = useNavigationContextSafe();
  const isOverlayMode = navContext?.navigationMode === 'overlay';
  const intl = useIntl();

  return (
    <Card className="rounded-lg">
      <CardHeader className="pb-0">
        <button
          onClick={() => setIsExpanded(!isExpanded)}
          className="w-full flex items-center justify-between text-left"
        >
          <div>
            <CardTitle className="mb-1">{intl.formatMessage(i18n.navigationTitle)}</CardTitle>
            <CardDescription>{intl.formatMessage(i18n.navigationDesc)}</CardDescription>
          </div>
          {isExpanded ? (
            <ChevronUp className="w-5 h-5 text-text-secondary" />
          ) : (
            <ChevronDown className="w-5 h-5 text-text-secondary" />
          )}
        </button>
      </CardHeader>
      {isExpanded && (
        <CardContent className="pt-4 px-4 space-y-6">
          <div>
            <h3 className="text-sm font-medium text-text-primary mb-3">
              {intl.formatMessage(i18n.navMode)}
            </h3>
            <NavigationModeSelector />
          </div>
          {!isOverlayMode && (
            <div>
              <h3 className="text-sm font-medium text-text-primary mb-3">
                {intl.formatMessage(i18n.navStyle)}
              </h3>
              <NavigationStyleSelector />
            </div>
          )}
          {!isOverlayMode && (
            <div>
              <h3 className="text-sm font-medium text-text-primary mb-3">
                {intl.formatMessage(i18n.navPosition)}
              </h3>
              <NavigationPositionSelector />
            </div>
          )}
          <div>
            <h3 className="text-sm font-medium text-text-primary mb-3">
              {intl.formatMessage(i18n.navCustomize)}
            </h3>
            <NavigationCustomizationSettings />
          </div>
        </CardContent>
      )}
    </Card>
  );
};

// Navigation Settings Card - wrapped in its own provider for settings page
const NavigationSettingsCard: React.FC = () => {
  const navContext = useNavigationContextSafe();

  // If already in a NavigationProvider context, render directly
  if (navContext) {
    return <NavigationSettingsContent />;
  }

  // Otherwise wrap with provider
  return (
    <NavigationProvider>
      <NavigationSettingsContent />
    </NavigationProvider>
  );
};

export default function AppSettingsSection({ scrollToSection }: AppSettingsSectionProps) {
  const [menuBarIconEnabled, setMenuBarIconEnabled] = useState(true);
  const [dockIconEnabled, setDockIconEnabled] = useState(true);
  const [wakelockEnabled, setWakelockEnabled] = useState(true);
  const [isMacOS, setIsMacOS] = useState(false);
  const [isDockSwitchDisabled, setIsDockSwitchDisabled] = useState(false);
  const [showNotificationModal, setShowNotificationModal] = useState(false);
  const [showPricing, setShowPricing] = useState(true);
  const [isDarkMode, setIsDarkMode] = useState(false);
  const updateSectionRef = useRef<HTMLDivElement>(null);
  const shouldShowUpdates = !window.appConfig.get('GOOSE_VERSION');

  useEffect(() => {
    setIsMacOS(window.electron.platform === 'darwin');
  }, []);

  useEffect(() => {
    const updateTheme = () => {
      setIsDarkMode(document.documentElement.classList.contains('dark'));
    };

    updateTheme();

    const observer = new MutationObserver(updateTheme);
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['class'],
    });

    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    window.electron.getSetting('showPricing').then(setShowPricing);
  }, []);

  useEffect(() => {
    if (scrollToSection === 'update' && updateSectionRef.current) {
      setTimeout(() => {
        updateSectionRef.current?.scrollIntoView({ behavior: 'smooth', block: 'center' });
      }, 100);
    }
  }, [scrollToSection]);

  useEffect(() => {
    window.electron.getMenuBarIconState().then((enabled) => {
      setMenuBarIconEnabled(enabled);
    });

    window.electron.getWakelockState().then((enabled) => {
      setWakelockEnabled(enabled);
    });

    if (isMacOS) {
      window.electron.getDockIconState().then((enabled) => {
        setDockIconEnabled(enabled);
      });
    }
  }, [isMacOS]);

  const handleMenuBarIconToggle = async () => {
    const newState = !menuBarIconEnabled;
    // If we're turning off the menu bar icon and the dock icon is hidden,
    // we need to show the dock icon to maintain accessibility
    if (!newState && !dockIconEnabled && isMacOS) {
      const success = await window.electron.setDockIcon(true);
      if (success) {
        setDockIconEnabled(true);
      }
    }
    const success = await window.electron.setMenuBarIcon(newState);
    if (success) {
      setMenuBarIconEnabled(newState);
      trackSettingToggled('menu_bar_icon', newState);
    }
  };

  const handleDockIconToggle = async () => {
    const newState = !dockIconEnabled;
    // If we're turning off the dock icon and the menu bar icon is hidden,
    // we need to show the menu bar icon to maintain accessibility
    if (!newState && !menuBarIconEnabled) {
      const success = await window.electron.setMenuBarIcon(true);
      if (success) {
        setMenuBarIconEnabled(true);
      }
    }

    // Disable the switch to prevent rapid toggling
    setIsDockSwitchDisabled(true);
    setTimeout(() => {
      setIsDockSwitchDisabled(false);
    }, 1000);

    // Set the dock icon state
    const success = await window.electron.setDockIcon(newState);
    if (success) {
      setDockIconEnabled(newState);
      trackSettingToggled('dock_icon', newState);
    }
  };

  const handleWakelockToggle = async () => {
    const newState = !wakelockEnabled;
    const success = await window.electron.setWakelock(newState);
    if (success) {
      setWakelockEnabled(newState);
      trackSettingToggled('prevent_sleep', newState);
    }
  };

  const handleShowPricingToggle = async (checked: boolean) => {
    setShowPricing(checked);
    await window.electron.setSetting('showPricing', checked);
    trackSettingToggled('cost_tracking', checked);
    // Trigger event for other components
    window.dispatchEvent(new CustomEvent('showPricingChanged'));
  };

  const intl = useIntl();

  return (
    <div className="space-y-4 pr-4 pb-8 mt-1">
      <Card className="rounded-lg">
        <CardHeader className="pb-0">
          <CardTitle className="">{intl.formatMessage(i18n.appearanceTitle)}</CardTitle>
          <CardDescription>{intl.formatMessage(i18n.appearanceDesc)}</CardDescription>
        </CardHeader>
        <CardContent className="pt-4 space-y-4 px-4">
          <div className="flex items-center justify-between">
            <div>
              <h3 className="text-text-primary text-xs">
                {intl.formatMessage(i18n.notifications)}
              </h3>
              <p className="text-xs text-text-secondary max-w-md mt-[2px]">
                {intl.formatMessage(i18n.notificationsDesc, {
                  link: (
                    <span
                      className="underline hover:cursor-pointer"
                      onClick={() => setShowNotificationModal(true)}
                    >
                      {intl.formatMessage(i18n.configGuide)}
                    </span>
                  ),
                })}
              </p>
            </div>
            <div className="flex items-center">
              <Button
                className="flex items-center gap-2 justify-center"
                variant="secondary"
                size="sm"
                onClick={async () => {
                  try {
                    await window.electron.openNotificationsSettings();
                  } catch (error) {
                    console.error('Failed to open notification settings:', error);
                  }
                }}
              >
                <Settings />
                {intl.formatMessage(i18n.openSettings)}
              </Button>
            </div>
          </div>

          <div className="flex items-center justify-between">
            <div>
              <h3 className="text-text-primary text-xs">{intl.formatMessage(i18n.menuBarIcon)}</h3>
              <p className="text-xs text-text-secondary max-w-md mt-[2px]">
                {intl.formatMessage(i18n.menuBarIconDesc)}
              </p>
            </div>
            <div className="flex items-center">
              <Switch
                checked={menuBarIconEnabled}
                onCheckedChange={handleMenuBarIconToggle}
                variant="mono"
              />
            </div>
          </div>

          {isMacOS && (
            <div className="flex items-center justify-between">
              <div>
                <h3 className="text-text-primary text-xs">{intl.formatMessage(i18n.dockIcon)}</h3>
                <p className="text-xs text-text-secondary max-w-md mt-[2px]">
                  {intl.formatMessage(i18n.dockIconDesc)}
                </p>
              </div>
              <div className="flex items-center">
                <Switch
                  disabled={isDockSwitchDisabled}
                  checked={dockIconEnabled}
                  onCheckedChange={handleDockIconToggle}
                  variant="mono"
                />
              </div>
            </div>
          )}

          {/* Prevent Sleep */}
          <div className="flex items-center justify-between">
            <div>
              <h3 className="text-text-primary text-xs">{intl.formatMessage(i18n.preventSleep)}</h3>
              <p className="text-xs text-text-secondary max-w-md mt-[2px]">
                {intl.formatMessage(i18n.preventSleepDesc)}
              </p>
            </div>
            <div className="flex items-center">
              <Switch
                checked={wakelockEnabled}
                onCheckedChange={handleWakelockToggle}
                variant="mono"
              />
            </div>
          </div>

          {/* Cost Tracking */}
          {COST_TRACKING_ENABLED && (
            <div className="flex items-center justify-between mb-4">
              <div>
                <h3 className="text-text-primary">{intl.formatMessage(i18n.costTracking)}</h3>
                <p className="text-xs text-text-secondary max-w-md mt-[2px]">
                  {intl.formatMessage(i18n.costTrackingDesc)}
                </p>
              </div>
              <div className="flex items-center">
                <Switch
                  checked={showPricing}
                  onCheckedChange={handleShowPricingToggle}
                  variant="mono"
                />
              </div>
            </div>
          )}
        </CardContent>
      </Card>

      <Card className="rounded-lg">
        <CardHeader className="pb-0">
          <CardTitle className="mb-1">{intl.formatMessage(i18n.themeTitle)}</CardTitle>
          <CardDescription>{intl.formatMessage(i18n.themeDesc)}</CardDescription>
        </CardHeader>
        <CardContent className="pt-4 px-4">
          <ThemeSelector className="w-auto" hideTitle horizontal />
        </CardContent>
      </Card>

      {/* Navigation Settings */}
      <NavigationSettingsCard />

      <TelemetrySettings />

      <Card className="rounded-lg">
        <CardHeader className="pb-0">
          <CardTitle className="mb-1">{intl.formatMessage(i18n.helpTitle)}</CardTitle>
          <CardDescription>{intl.formatMessage(i18n.helpDesc)}</CardDescription>
        </CardHeader>
        <CardContent className="pt-4 px-4">
          <div className="flex space-x-4">
            <Button
              onClick={() => {
                window.open(
                  'https://github.com/aaif-goose/goose/issues/new?template=bug_report.md',
                  '_blank'
                );
              }}
              variant="secondary"
              size="sm"
            >
              {intl.formatMessage(i18n.reportBug)}
            </Button>
            <Button
              onClick={() => {
                window.open(
                  'https://github.com/aaif-goose/goose/issues/new?template=feature_request.md',
                  '_blank'
                );
              }}
              variant="secondary"
              size="sm"
            >
              {intl.formatMessage(i18n.requestFeature)}
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* Version Section - only show if GOOSE_VERSION is set */}
      {!shouldShowUpdates && (
        <Card className="rounded-lg">
          <CardHeader className="pb-0">
            <CardTitle className="mb-1">{intl.formatMessage(i18n.versionTitle)}</CardTitle>
          </CardHeader>
          <CardContent className="pt-4 px-4">
            <div className="flex items-center gap-3">
              <img
                src={isDarkMode ? BlockLogoWhite : BlockLogoBlack}
                alt="Block Logo" // TODO: replace with AAIF logo asset
                className="h-8 w-auto"
              />
              <span className="text-2xl font-mono text-black dark:text-white">
                {String(window.appConfig.get('GOOSE_VERSION') || 'Development')}
              </span>
            </div>
          </CardContent>
        </Card>
      )}

      {/* Update Section - only show if GOOSE_VERSION is NOT set */}
      {UPDATES_ENABLED && shouldShowUpdates && (
        <div ref={updateSectionRef}>
          <Card className="rounded-lg">
            <CardHeader className="pb-0">
              <CardTitle className="mb-1">{intl.formatMessage(i18n.updatesTitle)}</CardTitle>
              <CardDescription>{intl.formatMessage(i18n.updatesDesc)}</CardDescription>
            </CardHeader>
            <CardContent className="px-4">
              <UpdateSection />
            </CardContent>
          </Card>
        </div>
      )}

      {/* Notification Instructions Modal */}
      <Dialog
        open={showNotificationModal}
        onOpenChange={(open) => !open && setShowNotificationModal(false)}
      >
        <DialogContent className="sm:max-w-[500px]">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Settings className="text-iconStandard" size={24} />
              {intl.formatMessage(i18n.notificationsModalTitle)}
            </DialogTitle>
          </DialogHeader>

          <div className="py-4">
            {/* OS-specific instructions */}
            {isMacOS ? (
              <div className="space-y-4">
                <p>{intl.formatMessage(i18n.notificationsMacInstructions)}</p>
                <ol className="list-decimal pl-5 space-y-2">
                  <li>{intl.formatMessage(i18n.notificationsMacStep1)}</li>
                  <li>{intl.formatMessage(i18n.notificationsMacStep2)}</li>
                  <li>{intl.formatMessage(i18n.notificationsMacStep3)}</li>
                  <li>{intl.formatMessage(i18n.notificationsMacStep4)}</li>
                </ol>
              </div>
            ) : (
              <div className="space-y-4">
                <p>{intl.formatMessage(i18n.notificationsWinInstructions)}</p>
                <ol className="list-decimal pl-5 space-y-2">
                  <li>{intl.formatMessage(i18n.notificationsWinStep1)}</li>
                  <li>{intl.formatMessage(i18n.notificationsWinStep2)}</li>
                  <li>{intl.formatMessage(i18n.notificationsWinStep3)}</li>
                  <li>{intl.formatMessage(i18n.notificationsWinStep4)}</li>
                </ol>
              </div>
            )}
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={() => setShowNotificationModal(false)}>
              {intl.formatMessage(i18n.close)}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
