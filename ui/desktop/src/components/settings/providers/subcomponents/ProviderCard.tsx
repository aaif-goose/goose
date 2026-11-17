import CardContainer from './CardContainer';
import CardHeader from './CardHeader';
import CardBody from './CardBody';
import DefaultCardButtons from './buttons/DefaultCardButtons';
import type { ProviderDetails } from '../../../../types/providers';
import { defineMessages, useIntl } from '../../../../i18n';

const i18n = defineMessages({
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
};

export const ProviderCard = function ProviderCard({
  provider,
  onConfigure,
  onLaunch,
  isOnboarding,
}: ProviderCardProps) {
  const intl = useIntl();
  const metadata = provider.metadata;

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
      grayedOut={!provider.is_configured && isOnboarding} // onboarding page will have grayed out cards if not configured
      onClick={handleCardClick}
      header={
        <CardHeader
          name={metadata.display_name || provider.name || intl.formatMessage(i18n.unknownProvider)}
          description={description}
          isConfigured={provider.is_configured}
        />
      }
      body={
        <CardBody>
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
