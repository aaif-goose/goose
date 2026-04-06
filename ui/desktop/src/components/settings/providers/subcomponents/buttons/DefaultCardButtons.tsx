import { ConfigureSettingsButton, RocketButton } from './CardButtons';
import { ProviderDetails } from '../../../../../api';
import { defineMessages, useIntl } from '../../../../../i18n';

const i18n = defineMessages({
  configureSettings: {
    id: 'defaultCardButtons.configureSettings',
    defaultMessage: 'Configure {name} settings',
  },
  editSettings: {
    id: 'defaultCardButtons.editSettings',
    defaultMessage: 'Edit {name} settings',
  },
  deleteSettings: {
    id: 'defaultCardButtons.deleteSettings',
    defaultMessage: 'Delete {name} settings',
  },
  getStarted: {
    id: 'defaultCardButtons.getStarted',
    defaultMessage: 'Get started with goose!',
  },
});

// can define other optional callbacks as needed
interface CardButtonsProps {
  provider: ProviderDetails;
  isOnboardingPage: boolean;
  onConfigure: (provider: ProviderDetails) => void;
  onLaunch: (provider: ProviderDetails) => void;
}

export default function DefaultCardButtons({
  provider,
  isOnboardingPage,
  onLaunch,
  onConfigure,
}: CardButtonsProps) {
  const intl = useIntl();
  const name = provider.metadata.display_name;

  return (
    <>
      {/*Set up an unconfigured provider */}
      {!provider.is_configured && (
        <ConfigureSettingsButton
          tooltip={intl.formatMessage(i18n.configureSettings, { name })}
          onClick={(e) => {
            e.stopPropagation();
            onConfigure(provider);
          }}
        />
      )}
      {/*show edit tooltip instead when hovering over button for configured providers*/}
      {provider.is_configured && !isOnboardingPage && (
        <ConfigureSettingsButton
          tooltip={intl.formatMessage(i18n.editSettings, { name })}
          onClick={(e) => {
            e.stopPropagation();
            onConfigure(provider);
          }}
        />
      )}
      {/*show Launch button for configured providers on onboarding page*/}
      {provider.is_configured && isOnboardingPage && (
        <RocketButton
          tooltip={intl.formatMessage(i18n.getStarted)}
          onClick={(e) => {
            e.stopPropagation();
            onLaunch(provider);
          }}
        />
      )}
    </>
  );
}
