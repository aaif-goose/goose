import { Button } from '../../ui/button';
import { RefreshCw } from 'lucide-react';
import { useConfig } from '../../ConfigContext';
import { View, ViewOptions } from '../../../utils/navigationUtils';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  resetButton: {
    id: 'resetProviderSection.resetButton',
    defaultMessage: 'Reset Provider and Model',
  },
  resetDescription: {
    id: 'resetProviderSection.resetDescription',
    defaultMessage: "This will clear your selected model and provider settings. If no defaults are available, you'll be taken to the welcome screen to set them up again.",
  },
});

interface ResetProviderSectionProps {
  setView: (view: View, viewOptions?: ViewOptions) => void;
}

export default function ResetProviderSection(_props: ResetProviderSectionProps) {
  const intl = useIntl();
  const { remove } = useConfig();

  const handleResetProvider = async () => {
    try {
      await remove('GOOSE_PROVIDER', false);
      await remove('GOOSE_MODEL', false);

      window.location.reload();
    } catch (error) {
      console.error('Failed to reset provider and model:', error);
    }
  };

  return (
    <div className="p-2">
      <Button
        onClick={handleResetProvider}
        variant="destructive"
        className="flex items-center justify-center gap-2"
      >
        <RefreshCw className="h-4 w-4" />
        {intl.formatMessage(i18n.resetButton)}
      </Button>
      <p className="text-xs text-text-secondary mt-2">
        {intl.formatMessage(i18n.resetDescription)}
      </p>
    </div>
  );
}
