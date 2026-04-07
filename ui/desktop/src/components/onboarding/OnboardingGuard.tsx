import { useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useConfig } from '../ConfigContext';
import { useModelAndProvider } from '../ModelAndProviderContext';
import { Goose } from '../icons';
import ProviderSelector from './ProviderSelector';
import OnboardingSuccess from './OnboardingSuccess';
import {
  trackOnboardingStarted,
  trackOnboardingCompleted,
  trackOnboardingProviderSelected,
  trackTelemetryPreference,
  setTelemetryEnabled as setAnalyticsTelemetryEnabled,
} from '../../utils/analytics';
import { defineMessages, useIntl } from '../../i18n';

const i18n = defineMessages({
  welcomeTitle: {
    id: 'onboardingGuard.welcomeTitle',
    defaultMessage: 'Welcome to goose',
  },
  welcomeDescription: {
    id: 'onboardingGuard.welcomeDescription',
    defaultMessage: 'Your local AI agent. Connect an AI model provider to get started.',
  },
});

const TELEMETRY_CONFIG_KEY = 'GOOSE_TELEMETRY_ENABLED';

interface OnboardingGuardProps {
  children: React.ReactNode;
}

export default function OnboardingGuard({ children }: OnboardingGuardProps) {
  const intl = useIntl();
  const navigate = useNavigate();
  const { read, upsert, getProviders } = useConfig();
  const { refreshCurrentModelAndProvider } = useModelAndProvider();

  const [isCheckingProvider, setIsCheckingProvider] = useState(true);
  const [hasProvider, setHasProvider] = useState(false);
  const [hasSelection, setHasSelection] = useState(false);
  const [configuredProvider, setConfiguredProvider] = useState<string | null>(null);
  const [configuredProviderDisplayName, setConfiguredProviderDisplayName] = useState<string | null>(
    null
  );
  const [configuredModel, setConfiguredModel] = useState<string | null>(null);
  const hasTrackedOnboardingStart = useRef(false);

  useEffect(() => {
    const checkProvider = async () => {
      try {
        const provider = ((await read('GOOSE_PROVIDER', false)) as string) || '';
        setHasProvider(provider.trim() !== '');
      } catch (error) {
        console.error('Error checking provider:', error);
        setHasProvider(false);
      } finally {
        setIsCheckingProvider(false);
      }
    };
    checkProvider();
  }, [read]);

  useEffect(() => {
    if (!isCheckingProvider && !hasProvider && !hasTrackedOnboardingStart.current) {
      trackOnboardingStarted();
      hasTrackedOnboardingStart.current = true;
    }
  }, [isCheckingProvider, hasProvider]);

  const handleConfigured = async (providerName: string, modelId?: string) => {
    trackOnboardingProviderSelected({ provider: providerName });
    await upsert('GOOSE_PROVIDER', providerName, false);
    const providers = await getProviders(true);
    const matchedProvider = providers.find((p) => p.name === providerName);
    if (modelId) {
      await upsert('GOOSE_MODEL', modelId, false);
      setConfiguredModel(modelId);
    } else if (matchedProvider) {
      await upsert('GOOSE_MODEL', matchedProvider.metadata.default_model, false);
      setConfiguredModel(matchedProvider.metadata.default_model);
    }
    await refreshCurrentModelAndProvider();
    setConfiguredProvider(providerName);
    setConfiguredProviderDisplayName(matchedProvider?.metadata.display_name || providerName);
  };

  const finishOnboarding = async (telemetryEnabled: boolean) => {
    try {
      await upsert(TELEMETRY_CONFIG_KEY, telemetryEnabled, false);
    } catch (error) {
      console.error('Failed to save telemetry preference:', error);
    }
    trackTelemetryPreference(telemetryEnabled, 'onboarding');
    if (configuredProvider) {
      trackOnboardingCompleted(configuredProvider, configuredModel ?? undefined);
    }
    if (!telemetryEnabled) {
      setAnalyticsTelemetryEnabled(false);
    }
    navigate('/', { replace: true });
    setHasProvider(true);
  };

  if (isCheckingProvider) {
    return null;
  }

  if (hasProvider) {
    return <>{children}</>;
  }

  if (configuredProviderDisplayName) {
    return (
      <OnboardingSuccess providerName={configuredProviderDisplayName} onFinish={finishOnboarding} />
    );
  }

  return (
    <div className="h-screen w-full bg-background-default overflow-hidden">
      <div className="h-full overflow-y-auto">
        <div
          className={`flex flex-col items-center p-4 pb-8 transition-all duration-500 ease-in-out ${hasSelection ? 'pt-8' : 'pt-[15vh]'}`}
        >
          <div className="max-w-2xl w-full mx-auto">
            <div
              className={`text-left transition-all duration-500 ease-in-out overflow-hidden ${hasSelection ? 'max-h-0 opacity-0 mb-0' : 'max-h-60 opacity-100 mb-8'}`}
            >
              <div className="mb-4">
                <Goose className="size-8" />
              </div>
              <h1 className="text-2xl sm:text-4xl font-light mb-3">{intl.formatMessage(i18n.welcomeTitle)}</h1>
              <p className="text-text-muted text-base sm:text-lg">
                {intl.formatMessage(i18n.welcomeDescription)}
              </p>
            </div>

            <ProviderSelector
              onConfigured={handleConfigured}
              onFirstSelection={() => setHasSelection(true)}
            />
          </div>
        </div>
      </div>
    </div>
  );
}
