import { useState, useEffect } from 'react';
import { ChevronDown } from 'lucide-react';
import { DictationProvider, getDictationConfig, DictationProviderStatus } from '../../../api';
import { useConfig } from '../../ConfigContext';
import { Input } from '../../ui/input';
import { Button } from '../../ui/button';
import { trackSettingToggled } from '../../../utils/analytics';
import { LocalModelManager } from './LocalModelManager';
import { MicrophoneSelector } from './MicrophoneSelector';
import { DICTATION_ALLOWED_PROVIDERS } from '../../../updates';
import { useFeatures } from '../../../contexts/FeaturesContext';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from '../../ui/dropdown-menu';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  voiceDictationProvider: {
    id: 'dictationSettings.voiceDictationProvider',
    defaultMessage: 'Voice Dictation Provider',
  },
  chooseVoiceConversion: {
    id: 'dictationSettings.chooseVoiceConversion',
    defaultMessage: 'Choose how voice is converted to text',
  },
  disabled: {
    id: 'dictationSettings.disabled',
    defaultMessage: 'Disabled',
  },
  notConfigured: {
    id: 'dictationSettings.notConfigured',
    defaultMessage: '(not configured)',
  },
  configureApiKey: {
    id: 'dictationSettings.configureApiKey',
    defaultMessage: 'Configure the API key in <b>{settingsPath}</b>',
  },
  configuredIn: {
    id: 'dictationSettings.configuredIn',
    defaultMessage: '✓ Configured in {settingsPath}',
  },
  apiKey: {
    id: 'dictationSettings.apiKey',
    defaultMessage: 'API Key',
  },
  requiredForTranscription: {
    id: 'dictationSettings.requiredForTranscription',
    defaultMessage: 'Required for transcription',
  },
  configured: {
    id: 'dictationSettings.configured',
    defaultMessage: '(Configured)',
  },
  updateApiKey: {
    id: 'dictationSettings.updateApiKey',
    defaultMessage: 'Update API Key',
  },
  addApiKey: {
    id: 'dictationSettings.addApiKey',
    defaultMessage: 'Add API Key',
  },
  removeApiKey: {
    id: 'dictationSettings.removeApiKey',
    defaultMessage: 'Remove API Key',
  },
  enterApiKey: {
    id: 'dictationSettings.enterApiKey',
    defaultMessage: 'Enter your API key',
  },
  save: {
    id: 'dictationSettings.save',
    defaultMessage: 'Save',
  },
  cancel: {
    id: 'dictationSettings.cancel',
    defaultMessage: 'Cancel',
  },
});

export const DictationSettings = () => {
  const intl = useIntl();
  const { localInference, isLoading: isFeaturesLoading } = useFeatures();
  const [provider, setProvider] = useState<DictationProvider | null>(null);
  const [providerStatuses, setProviderStatuses] = useState<Record<string, DictationProviderStatus>>(
    {}
  );
  const [preferredMic, setPreferredMic] = useState<string | null>(null);
  const [apiKey, setApiKey] = useState('');
  const [isEditingKey, setIsEditingKey] = useState(false);
  const { read, upsert, remove } = useConfig();

  const refreshStatuses = async () => {
    const audioConfig = await getDictationConfig();
    setProviderStatuses(audioConfig.data || {});
  };

  useEffect(() => {
    if (isFeaturesLoading) return;

    const loadSettings = async () => {
      const providerValue = await read('voice_dictation_provider', false);
      let loadedProvider: DictationProvider | null = (providerValue as DictationProvider) || null;

      if (
        DICTATION_ALLOWED_PROVIDERS &&
        loadedProvider &&
        !DICTATION_ALLOWED_PROVIDERS.includes(loadedProvider)
      ) {
        loadedProvider = null;
        await upsert('voice_dictation_provider', '', false);
      }

      if (!localInference && loadedProvider === 'local') {
        loadedProvider = null;
        await upsert('voice_dictation_provider', '', false);
      }

      setProvider(loadedProvider);

      const micValue = await read('voice_dictation_preferred_mic', false);
      setPreferredMic((micValue as string) || null);

      await refreshStatuses();
    };

    loadSettings();
  }, [read, upsert, localInference, isFeaturesLoading]);

  const handleProviderChange = (value: string) => {
    const newProvider = value === 'disabled' ? null : (value as DictationProvider);
    setProvider(newProvider);
    upsert('voice_dictation_provider', newProvider || '', false);
    trackSettingToggled('voice_dictation', newProvider !== null);
  };

  const handleMicChange = (deviceId: string | null) => {
    setPreferredMic(deviceId);
    upsert('voice_dictation_preferred_mic', deviceId || '', false);
  };

  const handleSaveKey = async () => {
    if (!provider) return;
    const providerConfig = providerStatuses[provider];
    if (!providerConfig || providerConfig.uses_provider_config) return;

    const trimmedKey = apiKey.trim();
    if (!trimmedKey) return;

    const keyName = providerConfig.config_key!;
    await upsert(keyName, trimmedKey, true);
    setApiKey('');
    setIsEditingKey(false);
    await refreshStatuses();
  };

  const handleRemoveKey = async () => {
    if (!provider) return;
    const providerConfig = providerStatuses[provider];
    if (!providerConfig || providerConfig.uses_provider_config) return;

    const keyName = providerConfig.config_key!;
    await remove(keyName, true);
    setApiKey('');
    setIsEditingKey(false);
    await refreshStatuses();
  };

  const handleCancelEdit = () => {
    setApiKey('');
    setIsEditingKey(false);
  };

  const getProviderLabel = (p: DictationProvider | null): string => {
    if (!p) return intl.formatMessage(i18n.disabled);
    return p.charAt(0).toUpperCase() + p.slice(1);
  };

  const visibleProviders = (Object.keys(providerStatuses) as DictationProvider[]).filter(
    (p) => !DICTATION_ALLOWED_PROVIDERS || DICTATION_ALLOWED_PROVIDERS.includes(p)
  );

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between py-2 px-2 hover:bg-background-secondary rounded-lg transition-all">
        <div>
          <h3 className="text-text-primary">{intl.formatMessage(i18n.voiceDictationProvider)}</h3>
          <p className="text-xs text-text-secondary max-w-md mt-[2px]">
            {intl.formatMessage(i18n.chooseVoiceConversion)}
          </p>
        </div>
        <DropdownMenu onOpenChange={(open) => open && refreshStatuses()}>
          <DropdownMenuTrigger className="flex items-center gap-2 px-3 py-1.5 text-sm border border-border-primary rounded-md hover:border-border-primary transition-colors text-text-primary bg-background-primary">
            {getProviderLabel(provider)}
            <ChevronDown className="w-4 h-4" />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-max min-w-[250px] max-w-[350px]">
            <DropdownMenuRadioGroup
              value={provider ?? 'disabled'}
              onValueChange={handleProviderChange}
            >
              <DropdownMenuRadioItem value="disabled">{intl.formatMessage(i18n.disabled)}</DropdownMenuRadioItem>
              {visibleProviders.map((p) => (
                <DropdownMenuRadioItem key={p} value={p}>
                  {getProviderLabel(p)}
                  {!providerStatuses[p]?.configured && (
                    <span className="text-xs ml-1 text-text-secondary">{intl.formatMessage(i18n.notConfigured)}</span>
                  )}
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      {provider && providerStatuses[provider] && (
        <>
          {provider === 'local' ? (
            <div className="py-2 px-2">
              <LocalModelManager />
            </div>
          ) : providerStatuses[provider].uses_provider_config ? (
            <div className="py-2 px-2 bg-background-secondary rounded-lg">
              {!providerStatuses[provider].configured ? (
                <p className="text-xs text-text-secondary">
                  {intl.formatMessage(i18n.configureApiKey, { settingsPath: providerStatuses[provider].settings_path, b: (chunks: React.ReactNode) => <b>{chunks}</b> })}
                </p>
              ) : (
                <p className="text-xs text-green-600">
                  {intl.formatMessage(i18n.configuredIn, { settingsPath: providerStatuses[provider].settings_path })}
                </p>
              )}
            </div>
          ) : (
            <div className="py-2 px-2 bg-background-secondary rounded-lg">
              <div className="mb-2">
                <h4 className="text-text-primary text-sm">{intl.formatMessage(i18n.apiKey)}</h4>
                <p className="text-xs text-text-secondary mt-[2px]">
                  {intl.formatMessage(i18n.requiredForTranscription)}
                  {providerStatuses[provider]?.configured && (
                    <span className="text-green-600 ml-2">{intl.formatMessage(i18n.configured)}</span>
                  )}
                </p>
              </div>

              {!isEditingKey ? (
                <div className="flex gap-2 flex-wrap">
                  <Button variant="outline" size="sm" onClick={() => setIsEditingKey(true)}>
                    {providerStatuses[provider]?.configured ? intl.formatMessage(i18n.updateApiKey) : intl.formatMessage(i18n.addApiKey)}
                  </Button>
                  {providerStatuses[provider]?.configured && (
                    <Button variant="destructive" size="sm" onClick={handleRemoveKey}>
                      {intl.formatMessage(i18n.removeApiKey)}
                    </Button>
                  )}
                </div>
              ) : (
                <div className="space-y-2">
                  <Input
                    type="password"
                    value={apiKey}
                    onChange={(e) => setApiKey(e.target.value)}
                    placeholder={intl.formatMessage(i18n.enterApiKey)}
                    className="max-w-md"
                    autoFocus
                  />
                  <div className="flex gap-2">
                    <Button size="sm" onClick={handleSaveKey}>
                      {intl.formatMessage(i18n.save)}
                    </Button>
                    <Button variant="outline" size="sm" onClick={handleCancelEdit}>
                      {intl.formatMessage(i18n.cancel)}
                    </Button>
                  </div>
                </div>
              )}
            </div>
          )}

          <MicrophoneSelector selectedDeviceId={preferredMic} onDeviceChange={handleMicChange} />
        </>
      )}
    </div>
  );
};
