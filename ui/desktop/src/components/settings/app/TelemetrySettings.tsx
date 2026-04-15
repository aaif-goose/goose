import { useState, useEffect, useCallback } from 'react';
import { Switch } from '../../ui/switch';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../../ui/card';
import { useConfig } from '../../ConfigContext';
import { TELEMETRY_UI_ENABLED } from '../../../updates';
import PrivacyInfoModal from '../../onboarding/PrivacyInfoModal';
import { toastService } from '../../../toasts';
import {
  setTelemetryEnabled as setAnalyticsTelemetryEnabled,
  trackTelemetryPreference,
} from '../../../utils/analytics';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  title: {
    id: 'telemetrySettings.title',
    defaultMessage: 'Privacy',
  },
  description: {
    id: 'telemetrySettings.description',
    defaultMessage: 'Control how your data is used',
  },
  toggleLabel: {
    id: 'telemetrySettings.toggleLabel',
    defaultMessage: 'Anonymous usage data',
  },
  toggleDescription: {
    id: 'telemetrySettings.toggleDescription',
    defaultMessage: 'Help improve goose by sharing anonymous usage statistics.',
  },
  learnMore: {
    id: 'telemetrySettings.learnMore',
    defaultMessage: 'Learn more',
  },
  configErrorTitle: {
    id: 'telemetrySettings.configErrorTitle',
    defaultMessage: 'Configuration Error',
  },
  loadError: {
    id: 'telemetrySettings.loadError',
    defaultMessage: 'Failed to load telemetry settings.',
  },
  updateError: {
    id: 'telemetrySettings.updateError',
    defaultMessage: 'Failed to update telemetry settings.',
  },
});

const TELEMETRY_CONFIG_KEY = 'GOOSE_TELEMETRY_ENABLED';

export default function TelemetrySettings() {
  const intl = useIntl();
  const { read, upsert } = useConfig();
  const [telemetryEnabled, setTelemetryEnabled] = useState(true);
  const [isLoading, setIsLoading] = useState(true);
  const [showModal, setShowModal] = useState(false);

  const loadTelemetryStatus = useCallback(async () => {
    try {
      const value = await read(TELEMETRY_CONFIG_KEY, false);
      setTelemetryEnabled(value === null ? true : Boolean(value));
    } catch (error) {
      console.error('Failed to load telemetry status:', error);
      toastService.error({
        title: intl.formatMessage(i18n.configErrorTitle),
        msg: intl.formatMessage(i18n.loadError),
        traceback: error instanceof Error ? error.stack || '' : '',
      });
    } finally {
      setIsLoading(false);
    }
  }, [read, intl]);

  useEffect(() => {
    loadTelemetryStatus();
  }, [loadTelemetryStatus]);

  const handleTelemetryToggle = async (checked: boolean) => {
    try {
      await upsert(TELEMETRY_CONFIG_KEY, checked, false);
      setTelemetryEnabled(checked);
      setAnalyticsTelemetryEnabled(checked);
      trackTelemetryPreference(checked, 'settings');
    } catch (error) {
      console.error('Failed to update telemetry status:', error);
      toastService.error({
        title: intl.formatMessage(i18n.configErrorTitle),
        msg: intl.formatMessage(i18n.updateError),
        traceback: error instanceof Error ? error.stack || '' : '',
      });
    }
  };

  const handleModalClose = () => {
    setShowModal(false);
    loadTelemetryStatus();
  };

  if (!TELEMETRY_UI_ENABLED) {
    return null;
  }

  const title = intl.formatMessage(i18n.title);
  const description = intl.formatMessage(i18n.description);
  const toggleLabel = intl.formatMessage(i18n.toggleLabel);
  const toggleDescription = intl.formatMessage(i18n.toggleDescription);

  const learnMoreLink = (
    <button
      onClick={() => setShowModal(true)}
      className="text-blue-600 dark:text-blue-400 hover:underline"
    >
      {intl.formatMessage(i18n.learnMore)}
    </button>
  );

  const toggle = (
    <Switch
      checked={telemetryEnabled}
      onCheckedChange={handleTelemetryToggle}
      disabled={isLoading}
      variant="mono"
    />
  );

  const modal = <PrivacyInfoModal isOpen={showModal} onClose={handleModalClose} />;

  const toggleRow = (
    <div className="flex items-center justify-between">
      <div>
        <h4 className="text-text-primary text-xs">{toggleLabel}</h4>
        <p className="text-xs text-text-secondary max-w-md mt-[2px]">
          {toggleDescription} {learnMoreLink}
        </p>
      </div>
      <div className="flex items-center">{toggle}</div>
    </div>
  );

  return (
    <>
      <Card className="rounded-lg">
        <CardHeader className="pb-0">
          <CardTitle className="mb-1">{title}</CardTitle>
          <CardDescription>{description}</CardDescription>
        </CardHeader>
        <CardContent className="pt-4 space-y-4 px-4">{toggleRow}</CardContent>
      </Card>
      {modal}
    </>
  );
}
