import { useEffect, useState } from 'react';
import { useConfig } from '../../ConfigContext';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  toolsLabel: {
    id: 'developerMode.toolsLabel',
    defaultMessage: 'Tools',
  },
  toolsDescription: {
    id: 'developerMode.toolsDescription',
    defaultMessage: 'Separate shell, file, tree, and image tools.',
  },
  pythonSessionLabel: {
    id: 'developerMode.pythonSessionLabel',
    defaultMessage: 'Python session',
  },
  pythonSessionDescription: {
    id: 'developerMode.pythonSessionDescription',
    defaultMessage:
      'One persistent Python session. Data stays in variables across calls, compaction, and restarts instead of in the conversation.',
  },
});

const CONFIG_KEY = 'GOOSE_DEVELOPER_MODE';
const DEFAULT_MODE = 'tools';

const developerModes = [
  { key: 'tools', label: i18n.toolsLabel, description: i18n.toolsDescription },
  {
    key: 'python_session',
    label: i18n.pythonSessionLabel,
    description: i18n.pythonSessionDescription,
  },
];

export const DeveloperModeSection = () => {
  const intl = useIntl();
  const { config, upsert } = useConfig();
  const configuredMode = config[CONFIG_KEY];
  const [currentMode, setCurrentMode] = useState(DEFAULT_MODE);

  useEffect(() => {
    setCurrentMode(typeof configuredMode === 'string' ? configuredMode : DEFAULT_MODE);
  }, [configuredMode]);

  const handleModeChange = async (mode: string) => {
    try {
      await upsert(CONFIG_KEY, mode, false);
      setCurrentMode(mode);
    } catch (error) {
      console.error('Error updating developer mode:', error);
    }
  };

  return (
    <div className="space-y-1">
      {developerModes.map((mode) => {
        const checked = currentMode === mode.key;
        return (
          <label
            key={mode.key}
            className={`group flex items-center justify-between text-sm text-text-primary py-2 px-2 rounded-lg transition-all hover:cursor-pointer ${checked ? 'bg-background-secondary' : 'bg-background-primary hover:bg-background-secondary'}`}
          >
            <div>
              <h3 className="text-text-primary">{intl.formatMessage(mode.label)}</h3>
              <p className="text-text-secondary mt-[2px]">{intl.formatMessage(mode.description)}</p>
            </div>
            <div className="relative flex items-center">
              <input
                type="radio"
                name="developer-mode"
                value={mode.key}
                checked={checked}
                onChange={() => handleModeChange(mode.key)}
                className="peer sr-only"
              />
              <div
                className="h-4 w-4 rounded-full border border-border-primary
                    peer-checked:border-[6px] peer-checked:border-black dark:peer-checked:border-white
                    peer-checked:bg-white dark:peer-checked:bg-black
                    transition-all duration-200 ease-in-out group-hover:border-border-primary"
              ></div>
            </div>
          </label>
        );
      })}
    </div>
  );
};
