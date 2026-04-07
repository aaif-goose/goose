import { useState } from 'react';
import { Button } from '../../../ui/button';
import { SwitchModelModal } from './SwitchModelModal';
import type { View } from '../../../../utils/navigationUtils';
import { shouldShowPredefinedModels } from '../predefinedModelsUtils';
import { defineMessages, useIntl } from '../../../../i18n';

const i18n = defineMessages({
  switchModels: {
    id: 'modelSettingsButtons.switchModels',
    defaultMessage: 'Switch models',
  },
  configureProviders: {
    id: 'modelSettingsButtons.configureProviders',
    defaultMessage: 'Configure providers',
  },
});

interface ConfigureModelButtonsProps {
  setView: (view: View) => void;
}

export default function ModelSettingsButtons({ setView }: ConfigureModelButtonsProps) {
  const intl = useIntl();
  const [isAddModelModalOpen, setIsAddModelModalOpen] = useState(false);
  const hasPredefinedModels = shouldShowPredefinedModels();

  return (
    <div className="flex gap-2 pt-4">
      <Button
        className="flex items-center gap-2 justify-center"
        variant="default"
        size="sm"
        onClick={() => setIsAddModelModalOpen(true)}
      >
        {intl.formatMessage(i18n.switchModels)}
      </Button>
      {isAddModelModalOpen ? (
        <SwitchModelModal
          sessionId={null}
          setView={setView}
          onClose={() => setIsAddModelModalOpen(false)}
        />
      ) : null}
      {!hasPredefinedModels && (
        <Button
          className="flex items-center gap-2 justify-center"
          variant="secondary"
          size="sm"
          onClick={() => {
            setView('ConfigureProviders');
          }}
        >
          {intl.formatMessage(i18n.configureProviders)}
        </Button>
      )}
    </div>
  );
}
