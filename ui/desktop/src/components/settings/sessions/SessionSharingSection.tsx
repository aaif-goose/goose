import React, { useState, useEffect } from 'react';
import { Input } from '../../ui/input';
import { Check, Lock, Loader2, AlertCircle } from 'lucide-react';
import { Switch } from '../../ui/switch';
import { Button } from '../../ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../../ui/card';
import { trackSettingToggled } from '../../../utils/analytics';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  title: {
    id: 'sessionSharingSection.title',
    defaultMessage: 'Session Sharing',
  },
  descriptionConfigured: {
    id: 'sessionSharingSection.descriptionConfigured',
    defaultMessage:
      'Session sharing is configured but fully opt-in — your sessions are only shared when you explicitly click the share button.',
  },
  descriptionDefault: {
    id: 'sessionSharingSection.descriptionDefault',
    defaultMessage: 'You can enable session sharing to share your sessions with others.',
  },
  alreadyConfigured: {
    id: 'sessionSharingSection.alreadyConfigured',
    defaultMessage: 'Session sharing has already been configured',
  },
  enableSharing: {
    id: 'sessionSharingSection.enableSharing',
    defaultMessage: 'Enable session sharing',
  },
  baseUrl: {
    id: 'sessionSharingSection.baseUrl',
    defaultMessage: 'Base URL',
  },
  urlPlaceholder: {
    id: 'sessionSharingSection.urlPlaceholder',
    defaultMessage: 'https://example.com/api',
  },
  invalidUrl: {
    id: 'sessionSharingSection.invalidUrl',
    defaultMessage:
      'Invalid URL format. Please enter a valid URL (e.g. https://example.com/api).',
  },
  testingConnection: {
    id: 'sessionSharingSection.testingConnection',
    defaultMessage: 'Testing connection...',
  },
  connectionSuccess: {
    id: 'sessionSharingSection.connectionSuccess',
    defaultMessage: 'Connection successful!',
  },
  serverError: {
    id: 'sessionSharingSection.serverError',
    defaultMessage:
      'Server error: HTTP {status}. The server may not be configured correctly.',
  },
  connectionFailed: {
    id: 'sessionSharingSection.connectionFailed',
    defaultMessage: 'Connection failed. ',
  },
  unreachableServer: {
    id: 'sessionSharingSection.unreachableServer',
    defaultMessage:
      'Unable to reach the server. Please check the URL and your network connection.',
  },
  connectionTimedOut: {
    id: 'sessionSharingSection.connectionTimedOut',
    defaultMessage: 'Connection timed out. The server may be slow or unreachable.',
  },
  unknownError: {
    id: 'sessionSharingSection.unknownError',
    defaultMessage: 'Unknown error occurred.',
  },
  testing: {
    id: 'sessionSharingSection.testing',
    defaultMessage: 'Testing...',
  },
  testConnection: {
    id: 'sessionSharingSection.testConnection',
    defaultMessage: 'Test Connection',
  },
});

export default function SessionSharingSection() {
  const intl = useIntl();
  const envBaseUrlShare = window.appConfig.get('GOOSE_BASE_URL_SHARE');

  // If env is set, force sharing enabled and set the baseUrl accordingly.
  const [sessionSharingConfig, setSessionSharingConfig] = useState({
    enabled: envBaseUrlShare ? true : false,
    baseUrl: typeof envBaseUrlShare === 'string' ? envBaseUrlShare : '',
  });
  const [urlError, setUrlError] = useState('');
  const [testResult, setTestResult] = useState<{
    status: 'success' | 'error' | 'testing' | null;
    message: string;
  }>({ status: null, message: '' });

  // isUrlConfigured is true if the user has configured a baseUrl and it is valid.
  const isUrlConfigured =
    !envBaseUrlShare &&
    sessionSharingConfig.enabled &&
    isValidUrl(String(sessionSharingConfig.baseUrl));

  // Only load saved config from settings if the env variable is not provided.
  useEffect(() => {
    if (envBaseUrlShare) {
      // If env variable is set, save the forced configuration to settings
      const forcedConfig = {
        enabled: true,
        baseUrl: typeof envBaseUrlShare === 'string' ? envBaseUrlShare : '',
      };
      window.electron.setSetting('sessionSharing', forcedConfig);
    } else {
      window.electron.getSetting('sessionSharing').then((config) => {
        setSessionSharingConfig(config);
      });
    }
  }, [envBaseUrlShare]);

  // Helper to check if the user's input is a valid URL
  function isValidUrl(value: string): boolean {
    if (!value) return false;
    try {
      new URL(value);
      return true;
    } catch {
      return false;
    }
  }

  // Toggle sharing (only allowed when env is not set).
  const toggleSharing = async () => {
    if (envBaseUrlShare) {
      return; // Do nothing if the environment variable forces sharing.
    }
    const updated = { ...sessionSharingConfig, enabled: !sessionSharingConfig.enabled };
    setSessionSharingConfig(updated);
    await window.electron.setSetting('sessionSharing', updated);
    trackSettingToggled('session_sharing', updated.enabled);
  };

  // Handle changes to the base URL field
  const handleBaseUrlChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const newBaseUrl = e.target.value;
    setSessionSharingConfig((prev) => ({
      ...prev,
      baseUrl: newBaseUrl,
    }));

    // Clear previous test results when URL changes
    setTestResult({ status: null, message: '' });

    if (isValidUrl(newBaseUrl)) {
      setUrlError('');
      const updated = { ...sessionSharingConfig, baseUrl: newBaseUrl };
      await window.electron.setSetting('sessionSharing', updated);
    } else {
      setUrlError(intl.formatMessage(i18n.invalidUrl));
    }
  };

  // Test connection to the configured URL
  const testConnection = async () => {
    const baseUrl = sessionSharingConfig.baseUrl;
    if (!baseUrl) return;

    setTestResult({ status: 'testing', message: intl.formatMessage(i18n.testingConnection) });

    try {
      // Create an AbortController for timeout
      const controller = new AbortController();
      const timeoutId = window.setTimeout(() => controller.abort(), 10000); // 10 second timeout

      const response = await fetch(baseUrl, {
        method: 'GET',
        headers: {
          Accept: 'application/json, text/plain, */*',
        },
        signal: controller.signal,
      });

      window.clearTimeout(timeoutId);

      // Consider any response (even 404) as a successful connection
      // since it means we can reach the server
      if (response.status < 500) {
        setTestResult({
          status: 'success',
          message: intl.formatMessage(i18n.connectionSuccess),
        });
      } else {
        setTestResult({
          status: 'error',
          message: intl.formatMessage(i18n.serverError, { status: response.status }),
        });
      }
    } catch (error) {
      console.error('Connection test failed:', error);
      let errorMessage = intl.formatMessage(i18n.connectionFailed);

      if (error instanceof TypeError && error.message.includes('fetch')) {
        errorMessage += intl.formatMessage(i18n.unreachableServer);
      } else if (error instanceof Error) {
        if (error.name === 'AbortError') {
          errorMessage += intl.formatMessage(i18n.connectionTimedOut);
        } else {
          errorMessage += error.message;
        }
      } else {
        errorMessage += intl.formatMessage(i18n.unknownError);
      }

      setTestResult({
        status: 'error',
        message: errorMessage,
      });
    }
  };

  return (
    <section id="session-sharing" className="space-y-4 pr-4 mt-1">
      <Card className="pb-2">
        <CardHeader className="pb-0">
          <CardTitle>{intl.formatMessage(i18n.title)}</CardTitle>
          <CardDescription>
            {(envBaseUrlShare as string)
              ? intl.formatMessage(i18n.descriptionConfigured)
              : intl.formatMessage(i18n.descriptionDefault)}
          </CardDescription>
        </CardHeader>
        <CardContent className="px-4 py-2">
          <div className="space-y-4">
            {/* Toggle for enabling session sharing */}
            <div className="flex items-center gap-3">
              <label className="text-sm cursor-pointer">
                {(envBaseUrlShare as string)
                  ? intl.formatMessage(i18n.alreadyConfigured)
                  : intl.formatMessage(i18n.enableSharing)}
              </label>

              {envBaseUrlShare ? (
                <Lock className="w-5 h-5 text-text-secondary" />
              ) : (
                <Switch
                  checked={sessionSharingConfig.enabled}
                  disabled={!!envBaseUrlShare}
                  onCheckedChange={toggleSharing}
                  variant="mono"
                />
              )}
            </div>

            {/* Base URL field (only visible if enabled) */}
            {sessionSharingConfig.enabled && (
              <div className="space-y-2 relative">
                <div className="flex items-center space-x-2">
                  <label htmlFor="session-sharing-url" className="text-sm text-text-primary">
                    {intl.formatMessage(i18n.baseUrl)}
                  </label>
                  {isUrlConfigured && <Check className="w-5 h-5 text-green-500" />}
                </div>
                <div className="flex items-center">
                  <Input
                    id="session-sharing-url"
                    type="url"
                    placeholder={intl.formatMessage(i18n.urlPlaceholder)}
                    value={sessionSharingConfig.baseUrl}
                    disabled={!!envBaseUrlShare}
                    {...(envBaseUrlShare ? {} : { onChange: handleBaseUrlChange })}
                  />
                </div>
                {urlError && <p className="text-red-500 text-sm">{urlError}</p>}

                {(isUrlConfigured || (envBaseUrlShare as string)) && (
                  <div className="space-y-2">
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={testConnection}
                      disabled={testResult.status === 'testing'}
                      className="flex items-center gap-2"
                    >
                      {testResult.status === 'testing' ? (
                        <>
                          <Loader2 className="w-4 h-4 animate-spin" />
                          {intl.formatMessage(i18n.testing)}
                        </>
                      ) : (
                        intl.formatMessage(i18n.testConnection)
                      )}
                    </Button>

                    {/* Test Results */}
                    {testResult.status && testResult.status !== 'testing' && (
                      <div
                        className={`flex items-start gap-2 p-3 rounded-md text-sm ${
                          testResult.status === 'success'
                            ? 'bg-green-50 text-green-800 border border-green-200'
                            : 'bg-red-50 text-red-800 border border-red-200'
                        }`}
                      >
                        {testResult.status === 'success' ? (
                          <Check className="w-4 h-4 mt-0.5 flex-shrink-0" />
                        ) : (
                          <AlertCircle className="w-4 h-4 mt-0.5 flex-shrink-0" />
                        )}
                        <span>{testResult.message}</span>
                      </div>
                    )}
                  </div>
                )}
              </div>
            )}
          </div>
        </CardContent>
      </Card>
    </section>
  );
}
