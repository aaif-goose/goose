import React from 'react';
import { Columns2, Layers } from 'lucide-react';
import { defineMessages, useIntl } from '../../../i18n';
import { useNavigationContext, NavigationMode } from '../../Layout/NavigationContext';
import { cn } from '../../../utils';

const i18n = defineMessages({
  pushLabel: {
    id: 'navigationModeSelector.pushLabel',
    defaultMessage: 'Push',
  },
  pushDescription: {
    id: 'navigationModeSelector.pushDescription',
    defaultMessage: 'Navigation pushes content',
  },
  overlayLabel: {
    id: 'navigationModeSelector.overlayLabel',
    defaultMessage: 'Overlay',
  },
  overlayDescription: {
    id: 'navigationModeSelector.overlayDescription',
    defaultMessage: 'Full-screen overlay',
  },
});

interface NavigationModeSelectorProps {
  className?: string;
}

export const NavigationModeSelector: React.FC<NavigationModeSelectorProps> = ({ className }) => {
  const { navigationMode, setNavigationMode } = useNavigationContext();
  const intl = useIntl();

  const modes: {
    value: NavigationMode;
    label: string;
    icon: React.ReactNode;
    description: string;
  }[] = [
    {
      value: 'push',
      label: intl.formatMessage(i18n.pushLabel),
      icon: <Columns2 className="w-5 h-5" />,
      description: intl.formatMessage(i18n.pushDescription),
    },
    {
      value: 'overlay',
      label: intl.formatMessage(i18n.overlayLabel),
      icon: <Layers className="w-5 h-5" />,
      description: intl.formatMessage(i18n.overlayDescription),
    },
  ];

  return (
    <div className={className}>
      <div className="grid grid-cols-2 gap-3">
        {modes.map((mode) => (
          <button
            key={mode.value}
            onClick={() => setNavigationMode(mode.value)}
            className={cn(
              'flex flex-col items-center gap-2 p-4 rounded-lg border-2 transition-all',
              navigationMode === mode.value
                ? 'border-border-primary bg-background-tertiary'
                : 'border-border-secondary bg-background-primary hover:border-border-medium'
            )}
          >
            <div className="text-text-primary">{mode.icon}</div>
            <div className="text-center">
              <div className="text-sm font-medium text-text-primary">{mode.label}</div>
              <div className="text-xs text-text-secondary mt-1">{mode.description}</div>
            </div>
          </button>
        ))}
      </div>
    </div>
  );
};
