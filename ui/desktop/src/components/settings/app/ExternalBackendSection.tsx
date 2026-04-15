import { useState, useEffect } from 'react';
import { Switch } from '../../ui/switch';
import { Input } from '../../ui/input';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../../ui/card';
import { AlertCircle } from 'lucide-react';
import { ExternalGoosedConfig, defaultSettings } from '../../../utils/settings';
import { WEB_PROTOCOLS } from '../../../utils/urlSecurity';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  title: {
    id: 'externalBackendSection.title',
    defaultMessage: 'Goose Server',
  },
  description: {
    id: 'externalBackendSection.description',
    defaultMessage:
      'By default goose launches a server for you, use this to connect to an external goose server',
  },
  useExternalServer: {
    id: 'externalBackendSection.useExternalServer',
    defaultMessage: 'Use external server',
  },
  useExternalServerDescription: {
    id: 'externalBackendSection.useExternalServerDescription',
    defaultMessage: 'Connect to a goose server running elsewhere (requires app restart)',
  },
  serverUrl: {
    id: 'externalBackendSection.serverUrl',
    defaultMessage: 'Server URL',
  },
  secretKey: {
    id: 'externalBackendSection.secretKey',
    defaultMessage: 'Secret Key',
  },
  secretKeyPlaceholder: {
    id: 'externalBackendSection.secretKeyPlaceholder',
    defaultMessage: "Enter the server's secret key",
  },
  secretKeyHelp: {
    id: 'externalBackendSection.secretKeyHelp',
    defaultMessage: 'The secret key configured on the goosed server (GOOSE_SERVER__SECRET_KEY)',
  },
  restartNote: {
    id: 'externalBackendSection.restartNote',
    defaultMessage:
      'Changes require restarting Goose to take effect. New chat windows will connect to the external server.',
  },
  urlProtocolError: {
    id: 'externalBackendSection.urlProtocolError',
    defaultMessage: 'URL must use http or https protocol',
  },
  urlFormatError: {
    id: 'externalBackendSection.urlFormatError',
    defaultMessage: 'Invalid URL format',
  },
});

export default function ExternalBackendSection() {
  const intl = useIntl();
  const [config, setConfig] = useState<ExternalGoosedConfig>(defaultSettings.externalGoosed);
  const [isSaving, setIsSaving] = useState(false);
  const [urlError, setUrlError] = useState<string | null>(null);

  useEffect(() => {
    const loadSettings = async () => {
      const externalGoosed = await window.electron.getSetting('externalGoosed');
      setConfig(externalGoosed);
    };
    loadSettings();
  }, []);

  const validateUrl = (value: string): boolean => {
    if (!value) {
      setUrlError(null);
      return true;
    }
    try {
      const parsed = new URL(value);
      if (!WEB_PROTOCOLS.includes(parsed.protocol)) {
        setUrlError(intl.formatMessage(i18n.urlProtocolError));
        return false;
      }
      setUrlError(null);
      return true;
    } catch {
      setUrlError(intl.formatMessage(i18n.urlFormatError));
      return false;
    }
  };

  const saveConfig = async (newConfig: ExternalGoosedConfig): Promise<void> => {
    setIsSaving(true);
    try {
      await window.electron.setSetting('externalGoosed', newConfig);
    } catch (error) {
      console.error('Failed to save external backend settings:', error);
    } finally {
      setIsSaving(false);
    }
  };

  const updateField = <K extends keyof ExternalGoosedConfig>(
    field: K,
    value: ExternalGoosedConfig[K]
  ) => {
    const newConfig = { ...config, [field]: value };
    setConfig(newConfig);
    return newConfig;
  };

  const handleUrlChange = (value: string) => {
    updateField('url', value);
    validateUrl(value);
  };

  const handleUrlBlur = async () => {
    if (validateUrl(config.url)) {
      await saveConfig(config);
    }
  };

  return (
    <section id="external-backend" className="space-y-4 pr-4 mt-1">
      <Card className="pb-2">
        <CardHeader className="pb-0">
          <CardTitle>{intl.formatMessage(i18n.title)}</CardTitle>
          <CardDescription>
            {intl.formatMessage(i18n.description)}
          </CardDescription>
        </CardHeader>
        <CardContent className="pt-4 space-y-4 px-4">
          <div className="flex items-center justify-between">
            <div>
              <h3 className="text-text-primary text-xs">{intl.formatMessage(i18n.useExternalServer)}</h3>
              <p className="text-xs text-text-secondary max-w-md mt-[2px]">
                {intl.formatMessage(i18n.useExternalServerDescription)}
              </p>
            </div>
            <div className="flex items-center">
              <Switch
                checked={config.enabled}
                onCheckedChange={(checked) => saveConfig(updateField('enabled', checked))}
                disabled={isSaving}
                variant="mono"
              />
            </div>
          </div>

          {config.enabled && (
            <>
              <div className="space-y-2">
                <label htmlFor="external-url" className="text-text-primary text-xs">
                  {intl.formatMessage(i18n.serverUrl)}
                </label>
                <Input
                  id="external-url"
                  type="url"
                  placeholder="http://127.0.0.1:3000"
                  value={config.url}
                  onChange={(e) => handleUrlChange(e.target.value)}
                  onBlur={handleUrlBlur}
                  disabled={isSaving}
                  className={urlError ? 'border-red-500' : ''}
                />
                {urlError && (
                  <p className="text-xs text-red-500 flex items-center gap-1">
                    <AlertCircle size={12} />
                    {urlError}
                  </p>
                )}
              </div>

              <div className="space-y-2">
                <label htmlFor="external-secret" className="text-text-primary text-xs">
                  {intl.formatMessage(i18n.secretKey)}
                </label>
                <Input
                  id="external-secret"
                  type="password"
                  placeholder={intl.formatMessage(i18n.secretKeyPlaceholder)}
                  value={config.secret}
                  onChange={(e) => updateField('secret', e.target.value)}
                  onBlur={() => saveConfig(config)}
                  disabled={isSaving}
                />
                <p className="text-xs text-text-secondary">
                  {intl.formatMessage(i18n.secretKeyHelp)}
                </p>
              </div>

              <div className="bg-amber-50 dark:bg-amber-950 border border-amber-200 dark:border-amber-800 rounded-md p-3">
                <p className="text-xs text-amber-800 dark:text-amber-200">
                  <strong>Note:</strong> {intl.formatMessage(i18n.restartNote)}
                </p>
              </div>
            </>
          )}
        </CardContent>
      </Card>
    </section>
  );
}
