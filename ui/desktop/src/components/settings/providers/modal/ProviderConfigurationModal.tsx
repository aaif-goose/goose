import { useState, Fragment } from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../../../ui/dialog';
import DefaultProviderSetupForm, {
  ConfigInput,
} from './subcomponents/forms/DefaultProviderSetupForm';
import ProviderSetupActions from './subcomponents/ProviderSetupActions';
import ProviderLogo from './subcomponents/ProviderLogo';
import { SecureStorageNotice } from './subcomponents/SecureStorageNotice';
import { providerConfigSubmitHandler } from './subcomponents/handlers/DefaultSubmitHandler';
import { useConfig } from '../../../ConfigContext';
import { useModelAndProvider } from '../../../ModelAndProviderContext';
import { AlertTriangle, LogIn } from 'lucide-react';
import {
  ProviderDetails,
  removeCustomProvider,
  configureProviderOauth,
  cleanupProviderCache,
} from '../../../../api';
import { Button } from '../../../../components/ui/button';
import { errorMessage } from '../../../../utils/conversionUtils';
import { defineMessages, useIntl } from '../../../../i18n';

const i18n = defineMessages({
  deleteConfigHeader: {
    id: 'providerConfigurationModal.deleteConfigHeader',
    defaultMessage: 'Delete configuration for {providerName}',
  },
  configureHeader: {
    id: 'providerConfigurationModal.configureHeader',
    defaultMessage: 'Configure {providerName}',
  },
  cannotDeleteActive: {
    id: 'providerConfigurationModal.cannotDeleteActive',
    defaultMessage:
      "You cannot delete this provider while it's currently in use. Please switch to a different model first.",
  },
  deleteConfirmation: {
    id: 'providerConfigurationModal.deleteConfirmation',
    defaultMessage: 'This will permanently delete the current provider configuration.',
  },
  oauthSignInDescription: {
    id: 'providerConfigurationModal.oauthSignInDescription',
    defaultMessage: 'Sign in with your {providerName} account to use this provider',
  },
  addApiKeyDescription: {
    id: 'providerConfigurationModal.addApiKeyDescription',
    defaultMessage: 'Add your API key(s) for this provider to integrate into goose',
  },
  oauthLoginFailed: {
    id: 'providerConfigurationModal.oauthLoginFailed',
    defaultMessage: 'OAuth login failed: {error}',
  },
  parameterRequired: {
    id: 'providerConfigurationModal.parameterRequired',
    defaultMessage: '{paramName} is required',
  },
  errorTitle: {
    id: 'providerConfigurationModal.errorTitle',
    defaultMessage: 'Error',
  },
  errorCheckingConfig: {
    id: 'providerConfigurationModal.errorCheckingConfig',
    defaultMessage: 'There was an error checking this provider configuration.',
  },
  checkConfigAgain: {
    id: 'providerConfigurationModal.checkConfigAgain',
    defaultMessage: 'Check your configuration again to use this provider.',
  },
  goBack: {
    id: 'providerConfigurationModal.goBack',
    defaultMessage: 'Go Back',
  },
  signingIn: {
    id: 'providerConfigurationModal.signingIn',
    defaultMessage: 'Signing in...',
  },
  signInWith: {
    id: 'providerConfigurationModal.signInWith',
    defaultMessage: 'Sign in with {providerName}',
  },
  browserWindowHint: {
    id: 'providerConfigurationModal.browserWindowHint',
    defaultMessage: 'A browser window will open for you to complete the login.',
  },
  deviceCodeFlowHint: {
    id: 'providerConfigurationModal.deviceCodeFlowHint',
    defaultMessage:
      'A browser window will open and the verification code will be copied to your clipboard. Paste it in the browser to complete sign-in.',
  },
  externalSetupIntro: {
    id: 'providerConfigurationModal.externalSetupIntro',
    defaultMessage: 'This provider is configured outside of goose. Follow these steps:',
  },
  seeDocumentation: {
    id: 'providerConfigurationModal.seeDocumentation',
    defaultMessage: 'See the <link>documentation</link> for more details.',
  },
  cancel: {
    id: 'providerConfigurationModal.cancel',
    defaultMessage: 'Cancel',
  },
  removeConfiguration: {
    id: 'providerConfigurationModal.removeConfiguration',
    defaultMessage: 'Remove Configuration',
  },
  close: {
    id: 'providerConfigurationModal.close',
    defaultMessage: 'Close',
  },
});

/** Render a setup step string, turning `backtick` spans into <code> and newlines into <br/>. */
function renderSetupStep(text: string) {
  // Split on backtick-wrapped segments
  const parts = text.split(/(`[^`]+`)/g);
  return parts.map((part, i) => {
    if (part.startsWith('`') && part.endsWith('`')) {
      return (
        <code
          key={i}
          className="px-1 py-0.5 rounded bg-background-secondary text-xs font-mono break-all"
        >
          {part.slice(1, -1)}
        </code>
      );
    }
    // Handle newlines within text
    const lines = part.split('\n');
    return (
      <Fragment key={i}>
        {lines.map((line, j) => (
          <Fragment key={j}>
            {j > 0 && <br />}
            {line}
          </Fragment>
        ))}
      </Fragment>
    );
  });
}

interface ProviderConfigurationModalProps {
  provider: ProviderDetails;
  onClose: () => void;
  onConfigured?: (provider: ProviderDetails) => void;
}

export default function ProviderConfigurationModal({
  provider,
  onClose,
  onConfigured,
}: ProviderConfigurationModalProps) {
  const intl = useIntl();
  const [validationErrors, setValidationErrors] = useState<Record<string, string>>({});
  const { upsert, remove } = useConfig();
  const { getCurrentModelAndProvider } = useModelAndProvider();
  const [configValues, setConfigValues] = useState<Record<string, ConfigInput>>({});
  const [showDeleteConfirmation, setShowDeleteConfirmation] = useState(false);
  const [isActiveProvider, setIsActiveProvider] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [isOAuthLoading, setIsOAuthLoading] = useState(false);

  let primaryParameters = provider.metadata.config_keys.filter((param) => param.primary);
  if (primaryParameters.length === 0) {
    primaryParameters = provider.metadata.config_keys;
  }

  const configKeys = provider.metadata.config_keys.filter((key) => !key.oauth_flow);
  const hasOAuth = provider.metadata.config_keys.some((key) => key.oauth_flow);
  const hasConfig = configKeys.length > 0;
  const hasDeviceCodeFlow = provider.metadata.config_keys.some((key) => key.device_code_flow);

  const isConfigured = provider.is_configured;
  const headerText = showDeleteConfirmation
    ? intl.formatMessage(i18n.deleteConfigHeader, { providerName: provider.metadata.display_name })
    : intl.formatMessage(i18n.configureHeader, { providerName: provider.metadata.display_name });

  const isExternalSetup =
    provider.metadata.config_keys.length === 0 &&
    provider.metadata.setup_steps &&
    provider.metadata.setup_steps.length > 0;

  const descriptionText = showDeleteConfirmation
    ? isActiveProvider
      ? intl.formatMessage(i18n.cannotDeleteActive)
      : intl.formatMessage(i18n.deleteConfirmation)
    : hasOAuth
      ? intl.formatMessage(i18n.oauthSignInDescription, {
          providerName: provider.metadata.display_name,
        })
      : isExternalSetup
        ? provider.metadata.description
        : intl.formatMessage(i18n.addApiKeyDescription);

  const handleOAuthLogin = async () => {
    setIsOAuthLoading(true);
    setError(null);
    try {
      if (hasConfig) {
        for (const key of configKeys) {
          const entry = configValues[key.name];
          const value =
            entry?.value ?? (typeof entry?.serverValue === 'string' ? entry.serverValue : null);
          if (value) {
            await upsert(key.name, value, key.secret);
          }
        }
      }
      await configureProviderOauth({
        path: { name: provider.name },
      });
      if (onConfigured) {
        onConfigured(provider);
      } else {
        onClose();
      }
    } catch (err) {
      setError(intl.formatMessage(i18n.oauthLoginFailed, { error: errorMessage(err) }));
    } finally {
      setIsOAuthLoading(false);
    }
  };

  const handleSubmitForm = async (e: React.FormEvent) => {
    e.preventDefault();

    setValidationErrors({});

    const parameters = provider.metadata.config_keys || [];
    const errors: Record<string, string> = {};

    parameters.forEach((parameter) => {
      if (
        parameter.required &&
        !configValues[parameter.name]?.value &&
        !configValues[parameter.name]?.serverValue
      ) {
        errors[parameter.name] = intl.formatMessage(i18n.parameterRequired, {
          paramName: parameter.name,
        });
      }
    });

    if (Object.keys(errors).length > 0) {
      setValidationErrors(errors);
      return;
    }

    const toSubmit = Object.fromEntries(
      Object.entries(configValues)
        .filter(
          ([_k, entry]) =>
            !!entry.value || (entry.serverValue != null && typeof entry.serverValue === 'string')
        )
        .map(([k, entry]) => [
          k,
          entry.value ?? (typeof entry.serverValue === 'string' ? entry.serverValue : ''),
        ])
    );

    try {
      await providerConfigSubmitHandler(upsert, provider, toSubmit);
      if (onConfigured) {
        onConfigured(provider);
      } else {
        onClose();
      }
    } catch (error) {
      setError(errorMessage(error));
    }
  };

  const handleCancel = () => {
    onClose();
  };

  const handleDelete = async () => {
    try {
      const providerModel = await getCurrentModelAndProvider();
      if (provider.name === providerModel.provider) {
        setIsActiveProvider(true);
        setShowDeleteConfirmation(true);
        return;
      }
    } catch (error) {
      console.error('Failed to check current provider:', error);
    }

    setIsActiveProvider(false);
    setShowDeleteConfirmation(true);
  };

  const handleConfirmDelete = async () => {
    if (isActiveProvider) {
      return;
    }

    // Clean up provider-specific cache files (e.g., OAuth tokens) before removing config
    try {
      await cleanupProviderCache({ path: { name: provider.name } });
    } catch {
      // Cleanup is best-effort — proceed with deletion even if it fails
    }

    const isCustomProvider = provider.provider_type === 'Custom';

    if (isCustomProvider) {
      await removeCustomProvider({
        path: { id: provider.name },
      });
    } else {
      const params = provider.metadata.config_keys;
      for (const param of params) {
        await remove(param.name, param.secret);
      }

      const hasOAuthKey = params.some((key) => key.oauth_flow);
      if (hasOAuthKey) {
        const configuredMarker = `${provider.name}_configured`;
        await remove(configuredMarker, false);
      }
    }

    onClose();
  };

  const getModalIcon = () => {
    if (showDeleteConfirmation) {
      return (
        <AlertTriangle
          className={isActiveProvider ? 'text-yellow-500' : 'text-red-500'}
          size={24}
        />
      );
    }
    return <ProviderLogo providerName={provider.name} />;
  };

  return (
    <>
      <Dialog open={!!error} onOpenChange={(open) => !open && setError(null)}>
        <DialogContent className="sm:max-w-[600px] max-h-[90vh] overflow-y-auto">
          <DialogTitle className="flex items-center gap-2">
            {intl.formatMessage(i18n.errorTitle)}
          </DialogTitle>
          <DialogDescription className="text-inherit text-base">
            {intl.formatMessage(i18n.errorCheckingConfig)}
          </DialogDescription>
          <pre className="ml-2">{error}</pre>
          <div>{intl.formatMessage(i18n.checkConfigAgain)}</div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setError(null)}>
              {intl.formatMessage(i18n.goBack)}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <Dialog open={!error} onOpenChange={(open) => !open && onClose()}>
        <DialogContent className="sm:max-w-[600px] max-h-[90vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              {getModalIcon()}
              {headerText}
            </DialogTitle>
            <DialogDescription>{descriptionText}</DialogDescription>
          </DialogHeader>

          <div className="py-4">
            {/* Contains information used to set up each provider */}
            {/* Only show the form when NOT in delete confirmation mode */}
            {!showDeleteConfirmation && (
              <>
                {hasOAuth && (
                  <div className="flex flex-col items-center gap-4 py-6">
                    <Button
                      onClick={handleOAuthLogin}
                      disabled={isOAuthLoading}
                      className="flex items-center gap-2 px-6 py-3"
                      size="lg"
                    >
                      <LogIn size={20} />
                      {isOAuthLoading
                        ? intl.formatMessage(i18n.signingIn)
                        : intl.formatMessage(i18n.signInWith, {
                            providerName: provider.metadata.display_name,
                          })}
                    </Button>
                    <p className="text-sm text-text-secondary text-center">
                      {hasDeviceCodeFlow
                        ? intl.formatMessage(i18n.deviceCodeFlowHint)
                        : intl.formatMessage(i18n.browserWindowHint)}
                    </p>
                  </div>
                )}

                {hasOAuth && hasConfig && (
                  <DefaultProviderSetupForm
                    configValues={configValues}
                    setConfigValues={setConfigValues}
                    provider={{
                      ...provider,
                      metadata: {
                        ...provider.metadata,
                        config_keys: configKeys,
                      },
                    }}
                    validationErrors={validationErrors}
                  />
                )}

                {isExternalSetup && (
                  <div className="space-y-3">
                    <p className="text-sm text-text-secondary">
                      {intl.formatMessage(i18n.externalSetupIntro)}
                    </p>
                    <ol className="ml-5 list-decimal text-sm text-text-primary space-y-2">
                      {provider.metadata.setup_steps?.map((step, i) => (
                        <li key={i}>{renderSetupStep(step)}</li>
                      ))}
                    </ol>
                    {provider.metadata.model_doc_link && (
                      <p className="text-sm text-text-secondary mt-4">
                        {intl.formatMessage(i18n.seeDocumentation, {
                          link: (chunks: React.ReactNode) => (
                            <a
                              href="#"
                              onClick={(e) => {
                                e.preventDefault();
                                window.electron.openExternal(provider.metadata.model_doc_link);
                              }}
                              className="underline hover:text-text-primary"
                            >
                              {chunks}
                            </a>
                          ),
                        })}
                      </p>
                    )}
                  </div>
                )}

                {!hasOAuth && !isExternalSetup && (
                  <>
                    <DefaultProviderSetupForm
                      configValues={configValues}
                      setConfigValues={setConfigValues}
                      provider={provider}
                      validationErrors={validationErrors}
                    />
                    {primaryParameters.length > 0 && provider.metadata.config_keys.length > 0 && (
                      <SecureStorageNotice />
                    )}
                  </>
                )}
              </>
            )}
          </div>

          <DialogFooter>
            {hasOAuth && !showDeleteConfirmation ? (
              <div className="flex gap-2 justify-end">
                <Button variant="outline" onClick={handleCancel}>
                  {intl.formatMessage(i18n.cancel)}
                </Button>
                {isConfigured && (
                  <Button variant="destructive" onClick={handleDelete}>
                    {intl.formatMessage(i18n.removeConfiguration)}
                  </Button>
                )}
              </div>
            ) : isExternalSetup && !showDeleteConfirmation ? (
              <div className="w-full">
                <Button
                  type="button"
                  variant="ghost"
                  onClick={handleCancel}
                  className="w-full h-[60px] rounded-none border-t border-border-primary text-md hover:bg-background-secondary text-text-primary font-medium"
                >
                  {intl.formatMessage(i18n.close)}
                </Button>
              </div>
            ) : (
              <ProviderSetupActions
                primaryParameters={primaryParameters}
                onCancel={handleCancel}
                onSubmit={handleSubmitForm}
                onDelete={handleDelete}
                showDeleteConfirmation={showDeleteConfirmation}
                onConfirmDelete={handleConfirmDelete}
                onCancelDelete={() => {
                  setIsActiveProvider(false);
                  setShowDeleteConfirmation(false);
                }}
                canDelete={isConfigured && !isActiveProvider}
                providerName={provider.metadata.display_name}
                isActiveProvider={isActiveProvider}
              />
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
