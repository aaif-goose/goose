import { useState, useEffect, useCallback } from 'react';
import { type MessageDescriptor } from 'react-intl';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../../ui/card';
import { Button } from '../../ui/button';
import { Switch } from '../../ui/switch';
import { ShortcutRecorder } from './ShortcutRecorder';
import { KeyboardShortcuts, defaultKeyboardShortcuts } from '../../../utils/settings';
import { trackSettingToggled } from '../../../utils/analytics';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  // Shortcut labels
  focusWindowLabel: {
    id: 'keyboardShortcuts.focusWindowLabel',
    defaultMessage: 'Focus Goose Window',
  },
  focusWindowDescription: {
    id: 'keyboardShortcuts.focusWindowDescription',
    defaultMessage: 'Bring Goose window to front from anywhere',
  },
  quickLauncherLabel: {
    id: 'keyboardShortcuts.quickLauncherLabel',
    defaultMessage: 'Quick Launcher',
  },
  quickLauncherDescription: {
    id: 'keyboardShortcuts.quickLauncherDescription',
    defaultMessage: 'Open the quick launcher overlay',
  },
  newChatLabel: {
    id: 'keyboardShortcuts.newChatLabel',
    defaultMessage: 'New Chat',
  },
  newChatDescription: {
    id: 'keyboardShortcuts.newChatDescription',
    defaultMessage: 'Create a new chat in the current window',
  },
  newChatWindowLabel: {
    id: 'keyboardShortcuts.newChatWindowLabel',
    defaultMessage: 'New Chat Window',
  },
  newChatWindowDescription: {
    id: 'keyboardShortcuts.newChatWindowDescription',
    defaultMessage: 'Open a new Goose window',
  },
  openDirectoryLabel: {
    id: 'keyboardShortcuts.openDirectoryLabel',
    defaultMessage: 'Open Directory',
  },
  openDirectoryDescription: {
    id: 'keyboardShortcuts.openDirectoryDescription',
    defaultMessage: 'Open directory selection dialog',
  },
  settingsLabel: {
    id: 'keyboardShortcuts.settingsLabel',
    defaultMessage: 'Settings',
  },
  settingsDescription: {
    id: 'keyboardShortcuts.settingsDescription',
    defaultMessage: 'Open settings panel',
  },
  findLabel: {
    id: 'keyboardShortcuts.findLabel',
    defaultMessage: 'Find',
  },
  findDescription: {
    id: 'keyboardShortcuts.findDescription',
    defaultMessage: 'Open search in conversation',
  },
  findNextLabel: {
    id: 'keyboardShortcuts.findNextLabel',
    defaultMessage: 'Find Next',
  },
  findNextDescription: {
    id: 'keyboardShortcuts.findNextDescription',
    defaultMessage: 'Jump to next search result',
  },
  findPreviousLabel: {
    id: 'keyboardShortcuts.findPreviousLabel',
    defaultMessage: 'Find Previous',
  },
  findPreviousDescription: {
    id: 'keyboardShortcuts.findPreviousDescription',
    defaultMessage: 'Jump to previous search result',
  },
  alwaysOnTopLabel: {
    id: 'keyboardShortcuts.alwaysOnTopLabel',
    defaultMessage: 'Always on Top',
  },
  alwaysOnTopDescription: {
    id: 'keyboardShortcuts.alwaysOnTopDescription',
    defaultMessage: 'Toggle window always on top',
  },
  toggleNavigationLabel: {
    id: 'keyboardShortcuts.toggleNavigationLabel',
    defaultMessage: 'Toggle Navigation',
  },
  toggleNavigationDescription: {
    id: 'keyboardShortcuts.toggleNavigationDescription',
    defaultMessage: 'Show or hide the navigation menu',
  },

  // Category labels and descriptions
  categoryGlobal: {
    id: 'keyboardShortcuts.categoryGlobal',
    defaultMessage: 'Global Shortcuts',
  },
  categoryGlobalDescription: {
    id: 'keyboardShortcuts.categoryGlobalDescription',
    defaultMessage: 'These shortcuts work system-wide, even when Goose is not focused',
  },
  categoryApplication: {
    id: 'keyboardShortcuts.categoryApplication',
    defaultMessage: 'Application Shortcuts',
  },
  categoryApplicationDescription: {
    id: 'keyboardShortcuts.categoryApplicationDescription',
    defaultMessage: 'These shortcuts work when Goose is the active application',
  },
  categorySearch: {
    id: 'keyboardShortcuts.categorySearch',
    defaultMessage: 'Search Shortcuts',
  },
  categorySearchDescription: {
    id: 'keyboardShortcuts.categorySearchDescription',
    defaultMessage: 'These shortcuts work when searching in a conversation',
  },
  categoryWindow: {
    id: 'keyboardShortcuts.categoryWindow',
    defaultMessage: 'Window Shortcuts',
  },
  categoryWindowDescription: {
    id: 'keyboardShortcuts.categoryWindowDescription',
    defaultMessage: 'These shortcuts control window behavior',
  },

  // UI strings
  loading: {
    id: 'keyboardShortcuts.loading',
    defaultMessage: 'Loading...',
  },
  restartRequired: {
    id: 'keyboardShortcuts.restartRequired',
    defaultMessage: 'Restart Required',
  },
  restartDescription: {
    id: 'keyboardShortcuts.restartDescription',
    defaultMessage:
      'Changes to application shortcuts (like New Chat, Settings, etc.) require restarting Goose to take effect. Global shortcuts (Focus Window, Quick Launcher) work immediately.',
  },
  dismiss: {
    id: 'keyboardShortcuts.dismiss',
    defaultMessage: 'Dismiss',
  },
  disabled: {
    id: 'keyboardShortcuts.disabled',
    defaultMessage: 'Disabled',
  },
  change: {
    id: 'keyboardShortcuts.change',
    defaultMessage: 'Change',
  },
  resetToDefaultsHeading: {
    id: 'keyboardShortcuts.resetToDefaultsHeading',
    defaultMessage: 'Reset to Defaults',
  },
  resetToDefaultsDescription: {
    id: 'keyboardShortcuts.resetToDefaultsDescription',
    defaultMessage: 'Restore all keyboard shortcuts to their original configuration',
  },
  resetAllShortcuts: {
    id: 'keyboardShortcuts.resetAllShortcuts',
    defaultMessage: 'Reset All Shortcuts',
  },

  // Dialog strings
  shortcutConflictTitle: {
    id: 'keyboardShortcuts.shortcutConflictTitle',
    defaultMessage: 'Shortcut Conflict',
  },
  shortcutConflictToggleMessage: {
    id: 'keyboardShortcuts.shortcutConflictToggleMessage',
    defaultMessage:
      'The shortcut {shortcut} is already assigned to "{conflictLabel}".',
  },
  shortcutConflictToggleDetail: {
    id: 'keyboardShortcuts.shortcutConflictToggleDetail',
    defaultMessage:
      'Enabling this will remove the shortcut from "{conflictLabel}" and assign it to "{targetLabel}". Do you want to continue?',
  },
  shortcutConflictSaveDetail: {
    id: 'keyboardShortcuts.shortcutConflictSaveDetail',
    defaultMessage:
      'Saving this will remove the shortcut from "{conflictLabel}" and assign it to "{targetLabel}". Do you want to continue?',
  },
  reassignShortcut: {
    id: 'keyboardShortcuts.reassignShortcut',
    defaultMessage: 'Reassign Shortcut',
  },
  cancel: {
    id: 'keyboardShortcuts.cancel',
    defaultMessage: 'Cancel',
  },
  resetShortcutsTitle: {
    id: 'keyboardShortcuts.resetShortcutsTitle',
    defaultMessage: 'Reset Keyboard Shortcuts',
  },
  resetShortcutsMessage: {
    id: 'keyboardShortcuts.resetShortcutsMessage',
    defaultMessage: 'Reset all keyboard shortcuts to their default values?',
  },
  resetShortcutsDetail: {
    id: 'keyboardShortcuts.resetShortcutsDetail',
    defaultMessage: 'This will restore all shortcuts to their original configuration.',
  },
});

interface ShortcutConfig {
  key: keyof KeyboardShortcuts;
  label: MessageDescriptor;
  description: MessageDescriptor;
  category: 'global' | 'application' | 'search' | 'window';
}

const shortcutConfigs: ShortcutConfig[] = [
  {
    key: 'focusWindow',
    label: i18n.focusWindowLabel,
    description: i18n.focusWindowDescription,
    category: 'global',
  },
  {
    key: 'quickLauncher',
    label: i18n.quickLauncherLabel,
    description: i18n.quickLauncherDescription,
    category: 'global',
  },
  {
    key: 'newChat',
    label: i18n.newChatLabel,
    description: i18n.newChatDescription,
    category: 'application',
  },
  {
    key: 'newChatWindow',
    label: i18n.newChatWindowLabel,
    description: i18n.newChatWindowDescription,
    category: 'application',
  },
  {
    key: 'openDirectory',
    label: i18n.openDirectoryLabel,
    description: i18n.openDirectoryDescription,
    category: 'application',
  },
  {
    key: 'settings',
    label: i18n.settingsLabel,
    description: i18n.settingsDescription,
    category: 'application',
  },
  {
    key: 'find',
    label: i18n.findLabel,
    description: i18n.findDescription,
    category: 'search',
  },
  {
    key: 'findNext',
    label: i18n.findNextLabel,
    description: i18n.findNextDescription,
    category: 'search',
  },
  {
    key: 'findPrevious',
    label: i18n.findPreviousLabel,
    description: i18n.findPreviousDescription,
    category: 'search',
  },
  {
    key: 'alwaysOnTop',
    label: i18n.alwaysOnTopLabel,
    description: i18n.alwaysOnTopDescription,
    category: 'window',
  },
  {
    key: 'toggleNavigation',
    label: i18n.toggleNavigationLabel,
    description: i18n.toggleNavigationDescription,
    category: 'application',
  },
];

const needsRestart = new Set<keyof KeyboardShortcuts>([
  'newChat',
  'newChatWindow',
  'openDirectory',
  'settings',
  'find',
  'findNext',
  'findPrevious',
  'alwaysOnTop',
]);

export const getShortcutLabel = (
  key: string,
  formatMessage: (descriptor: MessageDescriptor) => string
): string => {
  const config = shortcutConfigs.find((c) => c.key === key);
  return config ? formatMessage(config.label) : key;
};

export const formatShortcut = (shortcut: string): string => {
  const isMac = window.electron.platform === 'darwin';
  return shortcut
    .replace('CommandOrControl', isMac ? '⌘' : 'Ctrl')
    .replace('Command', '⌘')
    .replace('Control', 'Ctrl')
    .replace('Alt', isMac ? '⌥' : 'Alt')
    .replace('Shift', isMac ? '⇧' : 'Shift');
};

const categoryLabelMessages: Record<string, MessageDescriptor> = {
  global: i18n.categoryGlobal,
  application: i18n.categoryApplication,
  search: i18n.categorySearch,
  window: i18n.categoryWindow,
};

const categoryDescriptionMessages: Record<string, MessageDescriptor> = {
  global: i18n.categoryGlobalDescription,
  application: i18n.categoryApplicationDescription,
  search: i18n.categorySearchDescription,
  window: i18n.categoryWindowDescription,
};

export default function KeyboardShortcutsSection() {
  const intl = useIntl();
  const [shortcuts, setShortcuts] = useState<KeyboardShortcuts | null>(null);
  const [editingKey, setEditingKey] = useState<keyof KeyboardShortcuts | null>(null);
  const [showRestartNotice, setShowRestartNotice] = useState(false);

  const loadShortcuts = useCallback(async () => {
    const keyboardShortcuts = await window.electron.getSetting('keyboardShortcuts');
    setShortcuts({ ...defaultKeyboardShortcuts, ...keyboardShortcuts });
  }, []);

  useEffect(() => {
    loadShortcuts();
  }, [loadShortcuts]);

  const handleToggle = async (key: keyof KeyboardShortcuts, enabled: boolean) => {
    if (!shortcuts) return;

    const defaultValue = defaultKeyboardShortcuts[key];
    const newShortcuts = { ...shortcuts };

    if (enabled) {
      const conflictingKey = Object.entries(shortcuts).find(
        ([k, value]) => k !== key && value === defaultValue
      )?.[0];

      if (conflictingKey) {
        const confirmed = await window.electron.showMessageBox({
          type: 'warning',
          title: intl.formatMessage(i18n.shortcutConflictTitle),
          message: intl.formatMessage(i18n.shortcutConflictToggleMessage, {
            shortcut: formatShortcut(defaultValue),
            conflictLabel: getShortcutLabel(conflictingKey, intl.formatMessage),
          }),
          detail: intl.formatMessage(i18n.shortcutConflictToggleDetail, {
            conflictLabel: getShortcutLabel(conflictingKey, intl.formatMessage),
            targetLabel: getShortcutLabel(key, intl.formatMessage),
          }),
          buttons: [
            intl.formatMessage(i18n.reassignShortcut),
            intl.formatMessage(i18n.cancel),
          ],
          defaultId: 1,
        });

        if (confirmed.response !== 0) {
          return;
        }

        newShortcuts[conflictingKey as keyof KeyboardShortcuts] = null;
      }

      newShortcuts[key] = defaultValue;
    } else {
      newShortcuts[key] = null;
    }

    await window.electron.setSetting('keyboardShortcuts', newShortcuts);
    setShortcuts(newShortcuts);
    trackSettingToggled(`shortcut_${key}`, enabled);
    if (needsRestart.has(key)) {
      setShowRestartNotice(true);
    }
  };

  const handleEdit = (key: keyof KeyboardShortcuts) => {
    setEditingKey(key);
  };

  const handleSave = async (shortcut: string) => {
    if (!shortcuts || !editingKey) return;

    const conflictingKey = Object.entries(shortcuts).find(
      ([key, value]) => key !== editingKey && value === shortcut
    )?.[0];

    if (conflictingKey) {
      const confirmed = await window.electron.showMessageBox({
        type: 'warning',
        title: intl.formatMessage(i18n.shortcutConflictTitle),
        message: intl.formatMessage(i18n.shortcutConflictToggleMessage, {
          shortcut: formatShortcut(shortcut),
          conflictLabel: getShortcutLabel(conflictingKey, intl.formatMessage),
        }),
        detail: intl.formatMessage(i18n.shortcutConflictSaveDetail, {
          conflictLabel: getShortcutLabel(conflictingKey, intl.formatMessage),
          targetLabel: getShortcutLabel(editingKey, intl.formatMessage),
        }),
        buttons: [
          intl.formatMessage(i18n.reassignShortcut),
          intl.formatMessage(i18n.cancel),
        ],
        defaultId: 1,
      });

      if (confirmed.response !== 0) {
        return;
      }
    }

    const newShortcuts = { ...shortcuts };

    if (conflictingKey) {
      newShortcuts[conflictingKey as keyof KeyboardShortcuts] = null;
    }

    newShortcuts[editingKey] = shortcut || null;

    await window.electron.setSetting('keyboardShortcuts', newShortcuts);
    setShortcuts(newShortcuts);
    setEditingKey(null);
    if (needsRestart.has(editingKey)) {
      setShowRestartNotice(true);
    }
  };

  const handleCancel = () => {
    setEditingKey(null);
  };

  const handleResetToDefaults = async () => {
    const confirmed = await window.electron.showMessageBox({
      type: 'question',
      title: intl.formatMessage(i18n.resetShortcutsTitle),
      message: intl.formatMessage(i18n.resetShortcutsMessage),
      detail: intl.formatMessage(i18n.resetShortcutsDetail),
      buttons: [
        intl.formatMessage(i18n.resetToDefaultsHeading),
        intl.formatMessage(i18n.cancel),
      ],
      defaultId: 1,
    });

    if (confirmed.response === 0) {
      await window.electron.setSetting('keyboardShortcuts', { ...defaultKeyboardShortcuts });
      setShortcuts({ ...defaultKeyboardShortcuts });
      setShowRestartNotice(true);
      trackSettingToggled('shortcuts_reset', true);
    }
  };

  const groupedShortcuts = shortcutConfigs.reduce(
    (acc, config) => {
      if (!acc[config.category]) {
        acc[config.category] = [];
      }
      acc[config.category].push(config);
      return acc;
    },
    {} as Record<string, ShortcutConfig[]>
  );

  if (!shortcuts) {
    return <div>{intl.formatMessage(i18n.loading)}</div>;
  }

  return (
    <div className="space-y-4 pr-4 pb-8 mt-1">
      {showRestartNotice && (
        <Card className="rounded-lg border-yellow-600/50 bg-yellow-600/10">
          <CardContent className="pt-4 px-4 pb-4">
            <div className="flex items-start justify-between gap-4">
              <div className="flex-1">
                <h3 className="text-text-primary text-sm font-medium mb-1">
                  {intl.formatMessage(i18n.restartRequired)}
                </h3>
                <p className="text-xs text-text-secondary">
                  {intl.formatMessage(i18n.restartDescription)}
                </p>
              </div>
              <Button
                variant="secondary"
                size="sm"
                onClick={() => setShowRestartNotice(false)}
                className="text-xs shrink-0"
              >
                {intl.formatMessage(i18n.dismiss)}
              </Button>
            </div>
          </CardContent>
        </Card>
      )}
      {Object.entries(groupedShortcuts).map(([category, configs]) => (
        <Card key={category} className="rounded-lg">
          <CardHeader className="pb-0">
            <CardTitle>{intl.formatMessage(categoryLabelMessages[category])}</CardTitle>
            <CardDescription>
              {intl.formatMessage(categoryDescriptionMessages[category])}
            </CardDescription>
          </CardHeader>
          <CardContent className="pt-4 space-y-4 px-4">
            {configs.map((config) => {
              const shortcut = shortcuts[config.key];
              const isEditing = editingKey === config.key;

              return (
                <div key={config.key} className="flex items-center justify-between">
                  <div className="flex-1">
                    <h3 className="text-text-primary text-xs">
                      {intl.formatMessage(config.label)}
                    </h3>
                    <p className="text-xs text-text-secondary max-w-md mt-[2px]">
                      {intl.formatMessage(config.description)}
                    </p>
                  </div>
                  <div className="flex items-center gap-2">
                    {!isEditing ? (
                      <>
                        {shortcut ? (
                          <span className="text-xs font-mono px-2 py-1 bg-background-secondary rounded min-w-[120px] text-center">
                            {formatShortcut(shortcut)}
                          </span>
                        ) : (
                          <span className="text-xs text-text-secondary min-w-[120px] text-center">
                            {intl.formatMessage(i18n.disabled)}
                          </span>
                        )}
                        <Button
                          variant="secondary"
                          size="sm"
                          onClick={() => handleEdit(config.key)}
                          className="text-xs"
                        >
                          {intl.formatMessage(i18n.change)}
                        </Button>
                        <Switch
                          checked={shortcut !== null}
                          onCheckedChange={(checked) => handleToggle(config.key, checked)}
                          variant="mono"
                        />
                      </>
                    ) : (
                      <ShortcutRecorder
                        value={shortcut || ''}
                        onSave={handleSave}
                        onCancel={handleCancel}
                        allShortcuts={shortcuts}
                        currentKey={config.key}
                      />
                    )}
                  </div>
                </div>
              );
            })}
          </CardContent>
        </Card>
      ))}

      <Card className="rounded-lg">
        <CardContent className="pt-4 px-4 pb-4">
          <div className="flex items-center justify-between">
            <div>
              <h3 className="text-text-primary text-sm font-medium">
                {intl.formatMessage(i18n.resetToDefaultsHeading)}
              </h3>
              <p className="text-xs text-text-secondary max-w-md mt-[2px]">
                {intl.formatMessage(i18n.resetToDefaultsDescription)}
              </p>
            </div>
            <Button
              variant="secondary"
              size="sm"
              onClick={handleResetToDefaults}
              className="text-xs"
            >
              {intl.formatMessage(i18n.resetAllShortcuts)}
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
