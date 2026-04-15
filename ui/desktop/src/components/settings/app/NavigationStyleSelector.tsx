import React from 'react';
import { LayoutGrid, List } from 'lucide-react';
import { defineMessages, useIntl } from '../../../i18n';
import { useNavigationContext, NavigationStyle } from '../../Layout/NavigationContext';
import { cn } from '../../../utils';

const i18n = defineMessages({
  tileLabel: {
    id: 'navigationStyleSelector.tileLabel',
    defaultMessage: 'Tile',
  },
  tileDescription: {
    id: 'navigationStyleSelector.tileDescription',
    defaultMessage: 'Enlarged tile view',
  },
  listLabel: {
    id: 'navigationStyleSelector.listLabel',
    defaultMessage: 'List',
  },
  listDescription: {
    id: 'navigationStyleSelector.listDescription',
    defaultMessage: 'Classic condensed view',
  },
});

interface NavigationStyleSelectorProps {
  className?: string;
}

export const NavigationStyleSelector: React.FC<NavigationStyleSelectorProps> = ({ className }) => {
  const { navigationStyle, setNavigationStyle } = useNavigationContext();
  const intl = useIntl();

  const styles: {
    value: NavigationStyle;
    label: string;
    icon: React.ReactNode;
    description: string;
  }[] = [
    {
      value: 'expanded',
      label: intl.formatMessage(i18n.tileLabel),
      icon: <LayoutGrid className="w-5 h-5" />,
      description: intl.formatMessage(i18n.tileDescription),
    },
    {
      value: 'condensed',
      label: intl.formatMessage(i18n.listLabel),
      icon: <List className="w-5 h-5" />,
      description: intl.formatMessage(i18n.listDescription),
    },
  ];

  return (
    <div className={className}>
      <div className="grid grid-cols-2 gap-3">
        {styles.map((style) => (
          <button
            key={style.value}
            onClick={() => setNavigationStyle(style.value)}
            className={cn(
              'flex flex-col items-center gap-2 p-4 rounded-lg border-2 transition-all',
              navigationStyle === style.value
                ? 'border-border-primary bg-background-tertiary'
                : 'border-border-secondary bg-background-primary hover:border-border-medium'
            )}
          >
            <div className="text-text-primary">{style.icon}</div>
            <div className="text-center">
              <div className="text-sm font-medium text-text-primary">{style.label}</div>
              <div className="text-xs text-text-secondary mt-1">{style.description}</div>
            </div>
          </button>
        ))}
      </div>
    </div>
  );
};
