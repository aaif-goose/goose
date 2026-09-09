import { useState, useEffect } from 'react';
import { Switch } from '../../ui/switch';
import { Input } from '../../ui/input';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../../ui/card';
import { AlertCircle } from 'lucide-react';
import { ExternalBackendConfig, defaultSettings } from '../../../utils/settings';
import { defineMessages, useIntl } from '../../../i18n';
import { normalizeAcpHttpBaseUrl } from '../../../acp/url';

const i18n = defineMessages({
  title: {
    id: 'externalBackendSection.title',
    defaultMessage: 'External Backend (ACP)',
  },
  description: {
    id: 'externalBackendSection.description',
    defaultMessage:
      'By default Goose starts a local backend. Use this to connect to an external ACP-compatible backend.',
  },
  useExternalServer: {
    id: 'externalBackendSection.useExternalServer',
    defaultMessage: 'Use external backend',
  },
  useExternalServerDescription: {
    id: 'externalBackendSection.useExternalServerDescription',
    defaultMessage: 'Connect to an ACP-compatible backend running elsewhere.',
  },
  serverUrl: {
    id: 'externalBackendSection.serverUrl',
    defaultMessage: 'Backend Base URL',
  },
  serverUrlHelp: {
    id: 'externalBackendSection.serverUrlHelp',
    defaultMessage:
      'Enter the HTTP(S) base URL. Goose checks /status and connects to /acp under this base.',
  },
  workingDir: {
    id: 'externalBackendSection.workingDir',
    defaultMessage: 'Remote Working Directory (optional)',
  },
  workingDirPlaceholder: {
    id: 'externalBackendSection.workingDirPlaceholder',
    defaultMessage: '/home/goose/workspace',
  },
  workingDirHelp: {
    id: 'externalBackendSection.workingDirHelp',
    defaultMessage:
      'Absolute path on the external backend. Leave blank to send the local working directory.',
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
    defaultMessage: 'The secret key configured on the external backend (GOOSE_SERVER__SECRET_KEY).',
  },
  certFingerprint: {
    id: 'externalBackendSection.certFingerprint',
    defaultMessage: 'Certificate Fingerprint (optional)',
  },
  certFingerprintPlaceholder: {
    id: 'externalBackendSection.certFingerprintPlaceholder',
    defaultMessage: 'AA:BB:CC:... or sha256/base64',
  },
  certFingerprintHelp: {
    id: 'externalBackendSection.certFingerprintHelp',
    defaultMessage:
      'Pin a specific TLS certificate fingerprint. If omitted, the certificate is trusted on first use (TOFU).',
  },
  restartNote: {
    id: 'externalBackendSection.restartNote',
    defaultMessage: 'Changes apply to new chat windows. Restart Goose to update existing windows.',
  },
  urlProtocolError: {
    id: 'externalBackendSection.urlProtocolError',
    defaultMessage: 'URL must use http or https protocol',
  },
  fingerprintRequiresHttps: {
    id: 'externalBackendSection.fingerprintRequiresHttps',
    defaultMessage: 'Certificate fingerprint requires an https URL',
  },
  urlFormatError: {
    id: 'externalBackendSection.urlFormatError',
    defaultMessage: 'Invalid URL format',
  },
  urlBaseError: {
    id: 'externalBackendSection.urlBaseError',
    defaultMessage:
      'URL must be the backend base URL before /acp, without query parameters or fragments',
  },
});

export default function ExternalBackendSection() {
  const intl = useIntl();
  const [config, setConfig] = useState<ExternalBackendConfig>(defaultSettings.externalGoosed);
  const [isSaving, setIsSaving] = useState(false);
  const [urlError, setUrlError] = useState<string | null>(null);

  useEffect(() => {
    const loadSettings = async () => {
      const externalGoosed = await window.electron.getSetting('externalGoosed');
      setConfig(externalGoosed);
    };
    loadSettings();
  }, []);

  const validateUrl = (value: string, certFingerprint = config.certFingerprint): boolean => {
    if (!value) {
      setUrlError(null);
      return true;
    }
    try {
      const normalizedUrl = normalizeAcpHttpBaseUrl(value);
      const parsed = new URL(normalizedUrl);
      if (certFingerprint?.trim() && parsed.protocol !== 'https:') {
        setUrlError(intl.formatMessage(i18n.fingerprintRequiresHttps));
        return false;
      }
      setUrlError(null);
      return true;
    } catch (error) {
      const message = error instanceof Error ? error.message : '';
      if (message.includes('http: or https:')) {
        setUrlError(intl.formatMessage(i18n.urlProtocolError));
      } else if (
        message.includes('base URL before /acp') ||
        message.includes('query parameters or fragments')
      ) {
        setUrlError(intl.formatMessage(i18n.urlBaseError));
      } else {
        setUrlError(intl.formatMessage(i18n.urlFormatError));
      }
      return false;
    }
  };

  const saveConfig = async (newConfig: ExternalBackendConfig): Promise<void> => {
    setIsSaving(true);
    try {
      await window.electron.setSetting('externalGoosed', newConfig);
    } catch (error) {
      console.error('Failed to save external backend settings:', error);
    } finally {
      setIsSaving(false);
    }
  };

  const updateField = <K extends keyof ExternalBackendConfig>(
    field: K,
    value: ExternalBackendConfig[K]
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

  const handleCertFingerprintChange = (value: string) => {
    updateField('certFingerprint', value);
    validateUrl(config.url, value);
  };

  const handleCertFingerprintBlur = async () => {
    if (validateUrl(config.url)) {
      await saveConfig(config);
    }
  };

  return (
    <section id="external-backend" className="space-y-4 pr-4 mt-1">
      <Card className="pb-2">
        <CardHeader className="pb-0">
          <CardTitle>{intl.formatMessage(i18n.title)}</CardTitle>
          <CardDescription>{intl.formatMessage(i18n.description)}</CardDescription>
        </CardHeader>
        <CardContent className="pt-4 space-y-4 px-4">
          <div className="flex items-center justify-between">
            <div>
              <h3 className="text-text-primary text-xs">
                {intl.formatMessage(i18n.useExternalServer)}
              </h3>
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
                <p className="text-xs text-text-secondary">
                  {intl.formatMessage(i18n.serverUrlHelp)}
                </p>
              </div>

              <div className="space-y-2">
                <label htmlFor="external-working-dir" className="text-text-primary text-xs">
                  {intl.formatMessage(i18n.workingDir)}
                </label>
                <Input
                  id="external-working-dir"
                  type="text"
                  placeholder={intl.formatMessage(i18n.workingDirPlaceholder)}
                  value={config.workingDir || ''}
                  onChange={(e) => updateField('workingDir', e.target.value)}
                  onBlur={() => saveConfig(config)}
                  disabled={isSaving}
                />
                <p className="text-xs text-text-secondary">
                  {intl.formatMessage(i18n.workingDirHelp)}
                </p>
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

              <div className="space-y-2">
                <label htmlFor="external-cert-fingerprint" className="text-text-primary text-xs">
                  {intl.formatMessage(i18n.certFingerprint)}
                </label>
                <Input
                  id="external-cert-fingerprint"
                  type="text"
                  placeholder={intl.formatMessage(i18n.certFingerprintPlaceholder)}
                  value={config.certFingerprint || ''}
                  onChange={(e) => handleCertFingerprintChange(e.target.value)}
                  onBlur={handleCertFingerprintBlur}
                  disabled={isSaving}
                  className="font-mono text-xs"
                />
                <p className="text-xs text-text-secondary">
                  {intl.formatMessage(i18n.certFingerprintHelp)}
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
