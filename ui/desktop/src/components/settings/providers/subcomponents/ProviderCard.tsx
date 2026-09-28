import { useMemo, useState } from 'react';
import { Button } from '../../../ui/button';
import { acpSetProviderEnabled } from '../../../../acp/providers';
import { errorMessage } from '../../../../utils/conversionUtils';
import CardContainer from './CardContainer';
import CardHeader from './CardHeader';
import CardBody from './CardBody';
import DefaultCardButtons from './buttons/DefaultCardButtons';
import type { ProviderDetails, ProviderMetadata } from '../../../../types/providers';
import { defineMessages, useIntl } from '../../../../i18n';

const i18n = defineMessages({
  enable: { id: 'providerCard.enable', defaultMessage: 'Enable' },
  disable: { id: 'providerCard.disable', defaultMessage: 'Disable' },
  needsSetup: { id: 'providerCard.needsSetup', defaultMessage: 'Setup or sign-in required' },
  retainsSettings: {
    id: 'providerCard.retainsSettings',
    defaultMessage: 'Hide from the model picker and keep saved credentials and settings',
  },
  noMetadata: {
    id: 'providerCard.noMetadata',
    defaultMessage: 'ProviderCard error: No metadata provided',
  },
  unknownProvider: {
    id: 'providerCard.unknownProvider',
    defaultMessage: 'Unknown Provider',
  },
  deprecatedReplacement: {
    id: 'providerCard.deprecatedReplacement',
    defaultMessage: 'Deprecated — use {replacement} instead.',
  },
});

type ProviderCardProps = {
  provider: ProviderDetails;
  onConfigure: () => void;
  onLaunch: () => void;
  isOnboarding: boolean;
  onEnablementChanged?: () => void | Promise<void>;
};

export const ProviderCard = function ProviderCard({
  provider,
  onConfigure,
  onLaunch,
  isOnboarding,
  onEnablementChanged,
}: ProviderCardProps) {
  const intl = useIntl();
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const toggleEnabled = async () => {
    setSaving(true);
    setError(null);
    try {
      await acpSetProviderEnabled(provider.name, !provider.is_enabled);
      await onEnablementChanged?.();
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setSaving(false);
    }
  };
  // Safely access metadata with null checks
  const providerMetadata: ProviderMetadata | null = provider?.metadata || null;

  // Instead of useEffect for logging, use useMemo to memoize the metadata
  const metadata = useMemo(() => providerMetadata, [providerMetadata]);

  if (!metadata) {
    return <div>{intl.formatMessage(i18n.noMetadata)}</div>;
  }

  const handleCardClick = () => {
    if (!isOnboarding) {
      onConfigure();
    }
  };
  const description = provider.deprecated
    ? `${metadata.description} ${intl.formatMessage(i18n.deprecatedReplacement, {
        replacement: provider.replacement ?? intl.formatMessage(i18n.unknownProvider),
      })}`
    : metadata.description;

  return (
    <CardContainer
      testId={`provider-card-${provider.name.toLowerCase()}`}
      grayedOut={!provider.is_enabled && isOnboarding}
      onClick={handleCardClick}
      header={
        <CardHeader
          name={metadata.display_name || provider?.name || intl.formatMessage(i18n.unknownProvider)}
          description={description}
          isEnabled={provider.is_enabled}
        />
      }
      body={
        <CardBody>
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={saving}
            title={provider.is_enabled ? intl.formatMessage(i18n.retainsSettings) : undefined}
            aria-label={`${provider.is_enabled ? intl.formatMessage(i18n.disable) : intl.formatMessage(i18n.enable)} ${metadata.display_name}`}
            onClick={(event) => {
              event.stopPropagation();
              void toggleEnabled();
            }}
          >
            {provider.is_enabled
              ? intl.formatMessage(i18n.disable)
              : intl.formatMessage(i18n.enable)}
          </Button>
          {provider.is_enabled && !provider.is_configured && (
            <span className="text-xs text-text-secondary ml-2">
              {intl.formatMessage(i18n.needsSetup)}
            </span>
          )}
          {error && (
            <span role="alert" className="text-xs text-red-500 ml-2">
              {error}
            </span>
          )}
          <DefaultCardButtons
            provider={provider}
            onConfigure={onConfigure}
            onLaunch={onLaunch}
            isOnboardingPage={isOnboarding}
          />
        </CardBody>
      }
    />
  );
};
