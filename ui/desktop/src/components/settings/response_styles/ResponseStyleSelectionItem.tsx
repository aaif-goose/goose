import { useEffect, useState } from 'react';
import { defineMessages, useIntl } from '../../../i18n';
import type { MessageDescriptor } from 'react-intl';

const i18n = defineMessages({
  detailedLabel: {
    id: 'responseStyle.detailedLabel',
    defaultMessage: 'Detailed',
  },
  detailedDescription: {
    id: 'responseStyle.detailedDescription',
    defaultMessage: 'Tool calls are by default shown open to expose details',
  },
  conciseLabel: {
    id: 'responseStyle.conciseLabel',
    defaultMessage: 'Concise',
  },
  conciseDescription: {
    id: 'responseStyle.conciseDescription',
    defaultMessage: 'Tool calls are by default closed and only show the tool used',
  },
});

export interface ResponseStyle {
  key: string;
  label: MessageDescriptor;
  description: MessageDescriptor;
}

export const all_response_styles: ResponseStyle[] = [
  {
    key: 'detailed',
    label: i18n.detailedLabel,
    description: i18n.detailedDescription,
  },
  {
    key: 'concise',
    label: i18n.conciseLabel,
    description: i18n.conciseDescription,
  },
];

interface ResponseStyleSelectionItemProps {
  currentStyle: string;
  style: ResponseStyle;
  showDescription: boolean;
  handleStyleChange: (newStyle: string) => void;
}

export function ResponseStyleSelectionItem({
  currentStyle,
  style,
  showDescription,
  handleStyleChange,
}: ResponseStyleSelectionItemProps) {
  const intl = useIntl();
  const [checked, setChecked] = useState(currentStyle === style.key);

  useEffect(() => {
    setChecked(currentStyle === style.key);
  }, [currentStyle, style.key]);

  return (
    <div className="group hover:cursor-pointer text-sm">
      <div
        className={`flex items-center justify-between text-text-primary py-2 px-2 ${checked ? 'bg-background-secondary' : 'bg-background-primary hover:bg-background-secondary'} rounded-lg transition-all`}
        onClick={() => handleStyleChange(style.key)}
      >
        <div className="flex">
          <div>
            <h3 className="text-text-primary">{intl.formatMessage(style.label)}</h3>
            {showDescription && (
              <p className="text-xs text-text-secondary mt-[2px]">{intl.formatMessage(style.description)}</p>
            )}
          </div>
        </div>

        <div className="relative flex items-center gap-2">
          <input
            type="radio"
            name="responseStyles"
            value={style.key}
            checked={checked}
            onChange={() => handleStyleChange(style.key)}
            className="peer sr-only"
          />
          <div
            className="h-4 w-4 rounded-full border border-border-primary
                  peer-checked:border-[6px] peer-checked:border-black dark:peer-checked:border-white
                  peer-checked:bg-white dark:peer-checked:bg-black
                  transition-all duration-200 ease-in-out group-hover:border-border-primary"
          ></div>
        </div>
      </div>
    </div>
  );
}
