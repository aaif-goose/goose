import { useState } from 'react';
import { configureProviderOauth, ProviderDetails } from '../../api';
import { useConfig } from '../ConfigContext';
import DefaultProviderSetupForm, {
  ConfigInput,
} from '../settings/providers/modal/subcomponents/forms/DefaultProviderSetupForm';
import { providerConfigSubmitHandler } from '../settings/providers/modal/subcomponents/handlers/DefaultSubmitHandler';
import ProviderLogo from '../settings/providers/modal/subcomponents/ProviderLogo';
import { SecureStorageNotice } from '../settings/providers/modal/subcomponents/SecureStorageNotice';
import { Button } from '../ui/button';
import { LogIn, ChevronRight } from 'lucide-react';
import { defineMessages, useIntl } from '../../i18n';

const i18n = defineMessages({
  browserWindowOpen: {
    id: 'providerConfigForm.browserWindowOpen',
    defaultMessage: 'A browser window will open for you to complete the login.',
  },
  deviceCodeFlowHint: {
    id: 'providerConfigForm.deviceCodeFlowHint',
    defaultMessage:
      'A browser window will open and the verification code will be copied to your clipboard. Paste it in the browser to complete sign-in.',
  },
  signingIn: {
    id: 'providerConfigForm.signingIn',
    defaultMessage: 'Signing in...',
  },
  signInWith: {
    id: 'providerConfigForm.signInWith',
    defaultMessage: 'Sign in with {providerName}',
  },
  noApiKey: {
    id: 'providerConfigForm.noApiKey',
    defaultMessage: "Don't have an API key?",
  },
  configuring: {
    id: 'providerConfigForm.configuring',
    defaultMessage: 'Configuring...',
  },
  continue: {
    id: 'providerConfigForm.continue',
    defaultMessage: 'Continue',
  },
});

function parseLinks(text: string) {
  return text.split(/(https?:\/\/[^\s]+)/g).map((part, i) =>
    /^https?:\/\//.test(part) ? (
      <a
        key={i}
        href="#"
        onClick={(e) => {
          e.preventDefault();
          window.electron.openExternal(part);
        }}
        className="underline hover:text-text-default cursor-pointer"
      >
        {part}
      </a>
    ) : (
      part
    )
  );
}

function OAuthForm({
  provider,
  onConfigured,
  onError,
}: {
  provider: ProviderDetails;
  onConfigured: (name: string) => void;
  onError: (msg: string) => void;
}) {
  const intl = useIntl();
  const [isLoading, setIsLoading] = useState(false);

  const handleLogin = async () => {
    setIsLoading(true);
    try {
      await configureProviderOauth({
        path: { name: provider.name },
        throwOnError: true,
      });
      onConfigured(provider.name);
    } catch (err) {
      onError(`Sign-in failed: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setIsLoading(false);
    }
  };

  const isDeviceCodeFlow = provider.metadata.config_keys.some((key) => key.device_code_flow);

  return (
    <div className="flex flex-col items-center gap-3 py-4">
      <Button
        onClick={handleLogin}
        disabled={isLoading}
        className="flex items-center gap-2 px-6 py-3"
        size="lg"
      >
        <LogIn size={20} />
        {isLoading ? intl.formatMessage(i18n.signingIn) : intl.formatMessage(i18n.signInWith, { providerName: provider.metadata.display_name })}
      </Button>
      <p className="text-xs text-text-muted text-center">
        {isDeviceCodeFlow
          ? intl.formatMessage(i18n.deviceCodeFlowHint)
          : intl.formatMessage(i18n.browserWindowOpen)}
      </p>
    </div>
  );
}

function ApiKeyForm({
  provider,
  onConfigured,
  onError,
}: {
  provider: ProviderDetails;
  onConfigured: (name: string) => void;
  onError: (msg: string) => void;
}) {
  const intl = useIntl();
  const { upsert } = useConfig();
  const [configValues, setConfigValues] = useState<Record<string, ConfigInput>>({});
  const [validationErrors, setValidationErrors] = useState<Record<string, string>>({});
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [showSetupHelp, setShowSetupHelp] = useState(false);
  const setupSteps = provider.metadata.setup_steps;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setValidationErrors({});

    const parameters = provider.metadata.config_keys || [];
    const errors: Record<string, string> = {};
    parameters.forEach((param) => {
      if (
        param.required &&
        !configValues[param.name]?.value &&
        !configValues[param.name]?.serverValue
      ) {
        errors[param.name] = `${param.name} is required`;
      }
    });

    if (Object.keys(errors).length > 0) {
      setValidationErrors(errors);
      return;
    }

    const toSubmit = Object.fromEntries(
      Object.entries(configValues)
        .filter(([, entry]) => !!entry.value)
        .map(([k, entry]) => [k, entry.value || ''])
    );

    setIsSubmitting(true);
    try {
      await providerConfigSubmitHandler(upsert, provider, toSubmit);
      onConfigured(provider.name);
    } catch (err) {
      const msg =
        err instanceof Error
          ? err.message
          : typeof err === 'object' && err !== null && 'message' in err
            ? String((err as Record<string, unknown>).message)
            : JSON.stringify(err);
      onError(msg);
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <form onSubmit={handleSubmit}>
      <DefaultProviderSetupForm
        configValues={configValues}
        setConfigValues={setConfigValues}
        provider={provider}
        validationErrors={validationErrors}
        showOptions={false}
      />
      {provider.metadata.config_keys.some((k) => k.required && k.secret) && <SecureStorageNotice />}
      {setupSteps && setupSteps.length > 0 && (
        <div className="mt-3">
          <button
            type="button"
            onClick={() => setShowSetupHelp(!showSetupHelp)}
            className="flex items-center gap-1 text-sm text-text-muted hover:text-text-default transition-colors"
          >
            <ChevronRight
              size={14}
              className={`transition-transform duration-200 ${showSetupHelp ? 'rotate-90' : ''}`}
            />
            {intl.formatMessage(i18n.noApiKey)}
          </button>
          {showSetupHelp && (
            <ol className="mt-2 ml-5 list-decimal text-sm text-text-muted space-y-1">
              {setupSteps.map((step, i) => (
                <li key={i}>{parseLinks(step)}</li>
              ))}
            </ol>
          )}
        </div>
      )}
      <div className="mt-4">
        <Button type="submit" disabled={isSubmitting} className="w-full">
          {isSubmitting ? intl.formatMessage(i18n.configuring) : intl.formatMessage(i18n.continue)}
        </Button>
      </div>
    </form>
  );
}

interface ProviderConfigFormProps {
  provider: ProviderDetails;
  onConfigured: (providerName: string) => void;
}

export default function ProviderConfigForm({ provider, onConfigured }: ProviderConfigFormProps) {
  const [error, setError] = useState<string | null>(null);

  const isOAuthProvider = provider.metadata.config_keys.some((key) => key.oauth_flow);

  const renderForm = () => {
    if (isOAuthProvider) {
      return <OAuthForm provider={provider} onConfigured={onConfigured} onError={setError} />;
    }
    return <ApiKeyForm provider={provider} onConfigured={onConfigured} onError={setError} />;
  };

  return (
    <div>
      <div className="p-4 border rounded-xl bg-background-muted">
        <div className="flex items-center gap-3 mb-4">
          <ProviderLogo providerName={provider.name} />
          <div>
            <h3 className="font-medium text-text-default">{provider.metadata.display_name}</h3>
            <p className="text-xs text-text-muted">{provider.metadata.description}</p>
          </div>
        </div>

        {renderForm()}

        {error && (
          <div className="mt-3 p-3 rounded-lg bg-red-50 text-red-800 border border-red-200 dark:bg-red-900/20 dark:text-red-200 dark:border-red-800 text-sm">
            {error}
          </div>
        )}
      </div>
    </div>
  );
}
