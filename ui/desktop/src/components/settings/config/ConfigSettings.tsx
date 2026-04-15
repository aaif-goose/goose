import { useState, useEffect, useMemo } from 'react';
import { Input } from '../../ui/input';
import { Button } from '../../ui/button';
import { useConfig } from '../../ConfigContext';
import { cn } from '../../../utils';
import { Save, RotateCcw, FileText, Settings } from 'lucide-react';
import { toastSuccess, toastError } from '../../../toasts';
import { getUiNames, providerPrefixes } from '../../../utils/configUtils';
import type { ConfigData, ConfigValue } from '../../../types/config';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../../ui/card';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '../../ui/dialog';
import { errorMessage } from '../../../utils/conversionUtils';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  title: {
    id: 'configSettings.title',
    defaultMessage: 'Configuration',
  },
  description: {
    id: 'configSettings.description',
    defaultMessage: 'Edit your goose configuration settings',
  },
  descriptionWithProvider: {
    id: 'configSettings.descriptionWithProvider',
    defaultMessage: 'Edit your goose configuration settings (current settings for {provider})',
  },
  editConfiguration: {
    id: 'configSettings.editConfiguration',
    defaultMessage: 'Edit Configuration',
  },
  configurationEditor: {
    id: 'configSettings.configurationEditor',
    defaultMessage: 'Configuration Editor',
  },
  noSettings: {
    id: 'configSettings.noSettings',
    defaultMessage: 'No configuration settings found.',
  },
  enterValue: {
    id: 'configSettings.enterValue',
    defaultMessage: 'Enter {name}',
  },
  saving: {
    id: 'configSettings.saving',
    defaultMessage: 'Saving...',
  },
  resetChanges: {
    id: 'configSettings.resetChanges',
    defaultMessage: 'Reset Changes',
  },
  done: {
    id: 'configSettings.done',
    defaultMessage: 'Done',
  },
  configUpdated: {
    id: 'configSettings.configUpdated',
    defaultMessage: 'Configuration Updated',
  },
  configUpdatedMsg: {
    id: 'configSettings.configUpdatedMsg',
    defaultMessage: 'Successfully saved "{name}"',
  },
  saveFailed: {
    id: 'configSettings.saveFailed',
    defaultMessage: 'Save Failed',
  },
  saveFailedMsg: {
    id: 'configSettings.saveFailedMsg',
    defaultMessage: 'Failed to save "{name}"',
  },
  configReset: {
    id: 'configSettings.configReset',
    defaultMessage: 'Configuration Reset',
  },
  configResetMsg: {
    id: 'configSettings.configResetMsg',
    defaultMessage: 'All changes have been reverted',
  },
});

export default function ConfigSettings() {
  const intl = useIntl();
  const { config, upsert } = useConfig();
  const typedConfig = config as ConfigData;
  const [configValues, setConfigValues] = useState<ConfigData>({});
  const [modifiedKeys, setModifiedKeys] = useState<Set<string>>(new Set());
  const [saving, setSaving] = useState<string | null>(null);
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [originalKeyOrder, setOriginalKeyOrder] = useState<string[]>([]);

  useEffect(() => {
    setConfigValues(typedConfig);
    setModifiedKeys(new Set());

    // Capture the original key order only on first load or when new keys are added
    const currentKeys = Object.keys(typedConfig);
    setOriginalKeyOrder((prevOrder) => {
      if (prevOrder.length === 0) {
        // First load - capture the initial order
        return currentKeys;
      } else if (currentKeys.length > prevOrder.length) {
        // New keys have been added - add them to the end while preserving existing order
        const newKeys = currentKeys.filter((key) => !prevOrder.includes(key));
        return [...prevOrder, ...newKeys];
      }
      // Don't reorder when keys are just updated/saved - preserve the original order
      return prevOrder;
    });
  }, [typedConfig]);

  const handleChange = (key: string, value: string) => {
    setConfigValues((prev: ConfigData) => ({
      ...prev,
      [key]: value,
    }));

    setModifiedKeys((prev) => {
      const newSet = new Set(prev);
      if (value !== String(typedConfig[key] || '')) {
        newSet.add(key);
      } else {
        newSet.delete(key);
      }
      return newSet;
    });
  };

  const handleSave = async (key: string) => {
    setSaving(key);
    try {
      await upsert(key, configValues[key], false);
      toastSuccess({
        title: intl.formatMessage(i18n.configUpdated),
        msg: intl.formatMessage(i18n.configUpdatedMsg, { name: getUiNames(key) }),
      });

      // Remove this key from modified keys since it's now saved
      setModifiedKeys((prev) => {
        const newSet = new Set(prev);
        newSet.delete(key);
        return newSet;
      });
    } catch (error) {
      console.error('Failed to save config:', error);
      toastError({
        title: intl.formatMessage(i18n.saveFailed),
        msg: intl.formatMessage(i18n.saveFailedMsg, { name: getUiNames(key) }),
        traceback: errorMessage(error),
      });
    } finally {
      setSaving(null);
    }
  };

  const handleReset = () => {
    setConfigValues(typedConfig);
    setModifiedKeys(new Set());
    toastSuccess({
      title: intl.formatMessage(i18n.configReset),
      msg: intl.formatMessage(i18n.configResetMsg),
    });
  };

  const handleModalClose = (open: boolean) => {
    if (!open && modifiedKeys.size > 0) {
      // Reset any unsaved changes when closing the modal
      setConfigValues(typedConfig);
      setModifiedKeys(new Set());
    }
    setIsModalOpen(open);
  };

  const currentProvider = typedConfig.GOOSE_PROVIDER || '';

  const configEntries: [string, ConfigValue][] = useMemo(() => {
    const currentProviderPrefixes = providerPrefixes[currentProvider] || [];
    const allProviderPrefixes = Object.values(providerPrefixes).flat();

    return originalKeyOrder
      .filter((key) => {
        // skip secrets
        if (key === 'extensions' || key.includes('_KEY') || key.includes('_TOKEN')) {
          return false;
        }

        // Only show provider-specific entries for the current provider
        const providerSpecific = allProviderPrefixes.some((prefix: string) =>
          key.startsWith(prefix)
        );
        if (providerSpecific) {
          return currentProviderPrefixes.some((prefix: string) => key.startsWith(prefix));
        }

        return true;
      })
      .map((key) => [key, configValues[key]]);
  }, [originalKeyOrder, configValues, currentProvider]);

  return (
    <Card className="rounded-lg">
      <CardHeader className="pb-0">
        <CardTitle className="flex items-center gap-2">
          <FileText className="text-iconStandard" size={20} />
          {intl.formatMessage(i18n.title)}
        </CardTitle>
        <CardDescription>
          {currentProvider
            ? intl.formatMessage(i18n.descriptionWithProvider, { provider: currentProvider })
            : intl.formatMessage(i18n.description)}
        </CardDescription>
      </CardHeader>
      <CardContent className="pt-4 px-4">
        <Dialog open={isModalOpen} onOpenChange={handleModalClose}>
          <DialogTrigger asChild>
            <Button className="flex items-center gap-2" variant="secondary" size="sm">
              <Settings className="h-4 w-4" />
              {intl.formatMessage(i18n.editConfiguration)}
            </Button>
          </DialogTrigger>
          <DialogContent className="max-w-4xl max-h-[80vh]">
            <DialogHeader>
              <DialogTitle className="flex items-center gap-2">
                <FileText className="text-iconStandard" size={20} />
                {intl.formatMessage(i18n.configurationEditor)}
              </DialogTitle>
              <DialogDescription>
                {currentProvider
                  ? intl.formatMessage(i18n.descriptionWithProvider, { provider: currentProvider })
                  : intl.formatMessage(i18n.description)}
              </DialogDescription>
            </DialogHeader>

            <div className="flex-1 max-h-[60vh] overflow-auto pr-4">
              <div className="space-y-4">
                {configEntries.length === 0 ? (
                  <p className="text-text-secondary">{intl.formatMessage(i18n.noSettings)}</p>
                ) : (
                  configEntries.map(([key, _value]) => (
                    <div key={key} className="grid grid-cols-[200px_1fr_auto] gap-3 items-center">
                      <label className="text-sm font-medium text-text-primary" title={key}>
                        {getUiNames(key)}
                      </label>
                      <Input
                        value={String(configValues[key] || '')}
                        onChange={(e) => handleChange(key, e.target.value)}
                        className={cn(
                          'text-text-primary border-border-primary hover:border-border-primary transition-colors',
                          modifiedKeys.has(key) && 'border-blue-500 focus:ring-blue-500/20'
                        )}
                        placeholder={intl.formatMessage(i18n.enterValue, { name: getUiNames(key) })}
                      />
                      <Button
                        onClick={() => handleSave(key)}
                        disabled={!modifiedKeys.has(key) || saving === key}
                        variant="ghost"
                        size="sm"
                        className="min-w-[60px]"
                      >
                        {saving === key ? (
                          <span className="text-xs">{intl.formatMessage(i18n.saving)}</span>
                        ) : (
                          <Save className="h-4 w-4" />
                        )}
                      </Button>
                    </div>
                  ))
                )}
              </div>
            </div>

            <DialogFooter className="gap-2">
              {modifiedKeys.size > 0 && (
                <Button onClick={handleReset} variant="outline">
                  <RotateCcw className="h-4 w-4 mr-2" />
                  {intl.formatMessage(i18n.resetChanges)}
                </Button>
              )}
              <Button onClick={() => setIsModalOpen(false)} variant="default">
                {intl.formatMessage(i18n.done)}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      </CardContent>
    </Card>
  );
}
