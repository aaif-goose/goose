import React from 'react';
import { Moon, Sliders, Sun } from 'lucide-react';
import { Button } from '../ui/button';
import { useTheme } from '../../contexts/ThemeContext';
import { defineMessages, useIntl } from '../../i18n';

const i18n = defineMessages({
  theme: {
    id: 'themeSelector.theme',
    defaultMessage: 'Theme',
  },
  light: {
    id: 'themeSelector.light',
    defaultMessage: 'Light',
  },
  dark: {
    id: 'themeSelector.dark',
    defaultMessage: 'Dark',
  },
  system: {
    id: 'themeSelector.system',
    defaultMessage: 'System',
  },
});

interface ThemeSelectorProps {
  className?: string;
  hideTitle?: boolean;
  horizontal?: boolean;
}

const ThemeSelector: React.FC<ThemeSelectorProps> = ({
  className = '',
  hideTitle = false,
  horizontal = false,
}) => {
  const intl = useIntl();
  const { userThemePreference, setUserThemePreference } = useTheme();

  return (
    <div className={`${!horizontal ? 'px-1 py-2 space-y-2' : ''} ${className}`}>
      {!hideTitle && <div className="text-xs text-text-primary px-3">{intl.formatMessage(i18n.theme)}</div>}
      <div
        className={`${horizontal ? 'flex' : 'grid grid-cols-3'} gap-1 ${!horizontal ? 'px-3' : ''}`}
      >
        <Button
          data-testid="light-mode-button"
          onClick={() => setUserThemePreference('light')}
          className={`flex items-center justify-center gap-1 p-2 rounded-md border transition-colors text-xs ${
            userThemePreference === 'light'
              ? 'bg-background-inverse text-text-inverse border-text-inverse hover:!bg-background-inverse hover:!text-text-inverse'
              : 'border-border-primary hover:!bg-background-secondary text-text-secondary hover:text-text-primary'
          }`}
          variant="ghost"
          size="sm"
        >
          <Sun className="h-3 w-3" />
          <span>{intl.formatMessage(i18n.light)}</span>
        </Button>

        <Button
          data-testid="dark-mode-button"
          onClick={() => setUserThemePreference('dark')}
          className={`flex items-center justify-center gap-1 p-2 rounded-md border transition-colors text-xs ${
            userThemePreference === 'dark'
              ? 'bg-background-inverse text-text-inverse border-text-inverse hover:!bg-background-inverse hover:!text-text-inverse'
              : 'border-border-primary hover:!bg-background-secondary text-text-secondary hover:text-text-primary'
          }`}
          variant="ghost"
          size="sm"
        >
          <Moon className="h-3 w-3" />
          <span>{intl.formatMessage(i18n.dark)}</span>
        </Button>

        <Button
          data-testid="system-mode-button"
          onClick={() => setUserThemePreference('system')}
          className={`flex items-center justify-center gap-1 p-2 rounded-md border transition-colors text-xs ${
            userThemePreference === 'system'
              ? 'bg-background-inverse text-text-inverse border-text-inverse hover:!bg-background-inverse hover:!text-text-inverse'
              : 'border-border-primary hover:!bg-background-secondary text-text-secondary hover:text-text-primary'
          }`}
          variant="ghost"
          size="sm"
        >
          <Sliders className="h-3 w-3" />
          <span>{intl.formatMessage(i18n.system)}</span>
        </Button>
      </div>
    </div>
  );
};

export default ThemeSelector;
