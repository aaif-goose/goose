import { useState } from 'react';
import { Button } from '../ui/button';
import PrivacyInfoModal from './PrivacyInfoModal';
import { defineMessages, useIntl } from '../../i18n';

const LOCAL_PROVIDER = 'local';

const i18n = defineMessages({
  localModelReady: {
    id: 'onboardingSuccess.localModelReady',
    defaultMessage: 'Local model ready',
  },
  connectedTo: {
    id: 'onboardingSuccess.connectedTo',
    defaultMessage: 'Connected to {providerName}',
  },
  allSet: {
    id: 'onboardingSuccess.allSet',
    defaultMessage: "You're all set to start using goose.",
  },
  privacyTitle: {
    id: 'onboardingSuccess.privacyTitle',
    defaultMessage: 'Privacy',
  },
  privacyDescription: {
    id: 'onboardingSuccess.privacyDescription',
    defaultMessage: 'Anonymous usage data helps improve goose. We never collect your conversations, code, or personal data.',
  },
  learnMore: {
    id: 'onboardingSuccess.learnMore',
    defaultMessage: 'Learn more',
  },
  shareUsageData: {
    id: 'onboardingSuccess.shareUsageData',
    defaultMessage: 'Share anonymous usage data',
  },
  getStarted: {
    id: 'onboardingSuccess.getStarted',
    defaultMessage: 'Get Started',
  },
});

interface OnboardingSuccessProps {
  providerName: string;
  onFinish: (telemetryEnabled: boolean) => void;
}

export default function OnboardingSuccess({ providerName, onFinish }: OnboardingSuccessProps) {
  const intl = useIntl();
  const [showPrivacyInfo, setShowPrivacyInfo] = useState(false);
  const [telemetryOptIn, setTelemetryOptIn] = useState(true);

  return (
    <div className="h-screen w-full bg-background-default overflow-hidden">
      <div className="h-full overflow-y-auto">
        <div className="flex flex-col items-center justify-center h-full p-4">
          <div className="max-w-md w-full mx-auto text-center">
            <div className="mb-6">
              <div className="inline-flex items-center justify-center w-12 h-12 rounded-full bg-green-500/10 mb-4">
                <svg
                  className="w-6 h-6 text-green-500"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={2}
                    d="M5 13l4 4L19 7"
                  />
                </svg>
              </div>
              <h2 className="text-xl font-light text-text-default mb-1">
                {providerName === LOCAL_PROVIDER
                  ? intl.formatMessage(i18n.localModelReady)
                  : intl.formatMessage(i18n.connectedTo, { providerName })}
              </h2>
              <p className="text-text-muted text-sm">{intl.formatMessage(i18n.allSet)}</p>
            </div>

            <div className="w-full p-4 bg-transparent border rounded-xl text-left mb-6">
              <h3 className="font-medium text-text-default text-sm mb-1">{intl.formatMessage(i18n.privacyTitle)}</h3>
              <p className="text-text-muted text-sm">
                {intl.formatMessage(i18n.privacyDescription)}{' '}
                <button
                  onClick={() => setShowPrivacyInfo(true)}
                  className="text-blue-600 dark:text-blue-400 hover:underline"
                >
                  {intl.formatMessage(i18n.learnMore)}
                </button>
              </p>
              <label className="mt-3 flex items-center gap-2 cursor-pointer">
                <input
                  type="checkbox"
                  checked={telemetryOptIn}
                  onChange={(e) => setTelemetryOptIn(e.target.checked)}
                  className="rounded"
                />
                <span className="text-text-muted text-sm">{intl.formatMessage(i18n.shareUsageData)}</span>
              </label>
            </div>

            <Button onClick={() => onFinish(telemetryOptIn)} className="w-full">
              {intl.formatMessage(i18n.getStarted)}
            </Button>
          </div>
        </div>
      </div>

      <PrivacyInfoModal isOpen={showPrivacyInfo} onClose={() => setShowPrivacyInfo(false)} />
    </div>
  );
}
