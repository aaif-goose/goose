import { useEffect, useState } from 'react';
import { ChevronDown } from 'lucide-react';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from '../../ui/dropdown-menu';
import { AppEvents } from '../../../constants/events';
import { defineMessages, useIntl } from '../../../i18n';
import type { ShowThinking } from '../../../utils/settings';

const i18n = defineMessages({
  title: {
    id: 'showThinkingSetting.title',
    defaultMessage: 'Show Thinking',
  },
  description: {
    id: 'showThinkingSetting.description',
    defaultMessage:
      'Show the model\'s thinking in chat. "Collapsed" keeps it closed until you open it.',
  },
  always: {
    id: 'showThinkingSetting.always',
    defaultMessage: 'Always',
  },
  collapsed: {
    id: 'showThinkingSetting.collapsed',
    defaultMessage: 'Collapsed',
  },
  never: {
    id: 'showThinkingSetting.never',
    defaultMessage: 'Never',
  },
});

const options: ShowThinking[] = ['always', 'collapsed', 'never'];

export const ShowThinkingSetting = () => {
  const intl = useIntl();
  const [showThinking, setShowThinking] = useState<ShowThinking>('collapsed');

  useEffect(() => {
    // settings.json can be edited by hand; show unknown values as 'collapsed', like ThinkingContent does.
    window.electron.getSetting('showThinking').then((value) => {
      setShowThinking(options.includes(value) ? value : 'collapsed');
    });
  }, []);

  const handleChange = async (value: string) => {
    const newValue = value as ShowThinking;
    setShowThinking(newValue);
    await window.electron.setSetting('showThinking', newValue);
    window.dispatchEvent(new CustomEvent(AppEvents.SHOW_THINKING_CHANGED));
  };

  return (
    <div className="flex items-center justify-between py-2 px-2 hover:bg-background-secondary rounded-lg transition-all">
      <div>
        <h3 className="text-text-primary">{intl.formatMessage(i18n.title)}</h3>
        <p className="text-xs text-text-secondary max-w-md mt-[2px]">
          {intl.formatMessage(i18n.description)}
        </p>
      </div>
      <DropdownMenu>
        <DropdownMenuTrigger className="flex items-center gap-2 px-3 py-1.5 text-sm border border-border-primary rounded-md hover:border-border-primary transition-colors text-text-primary bg-background-primary">
          {intl.formatMessage(i18n[showThinking])}
          <ChevronDown className="w-4 h-4" />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuRadioGroup value={showThinking} onValueChange={handleChange}>
            {options.map((option) => (
              <DropdownMenuRadioItem key={option} value={option}>
                {intl.formatMessage(i18n[option])}
              </DropdownMenuRadioItem>
            ))}
          </DropdownMenuRadioGroup>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
};
