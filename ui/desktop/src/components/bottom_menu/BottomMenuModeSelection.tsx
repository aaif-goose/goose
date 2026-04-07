import { useEffect, useState } from 'react';
import { Tornado } from 'lucide-react';
import { all_goose_modes, ModeSelectionItem } from '../settings/mode/ModeSelectionItem';
import { useConfig } from '../ConfigContext';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '../ui/dropdown-menu';
import { trackModeChanged } from '../../utils/analytics';
import { getSession, updateSession } from '../../api';
import { defineMessages, useIntl } from '../../i18n';

const i18n = defineMessages({
  autoFallback: {
    id: 'bottomMenuModeSelection.autoFallback',
    defaultMessage: 'auto',
  },
  automaticModeDescription: {
    id: 'bottomMenuModeSelection.automaticModeDescription',
    defaultMessage: 'Automatic mode selection',
  },
  currentModeTitle: {
    id: 'bottomMenuModeSelection.currentModeTitle',
    defaultMessage: 'Current mode: {label} - {description}',
  },
});

export const BottomMenuModeSelection = ({ sessionId }: { sessionId: string | null }) => {
  const intl = useIntl();
  const [gooseMode, setGooseMode] = useState('auto');
  const { config } = useConfig();

  useEffect(() => {
    let cancelled = false;
    if (sessionId) {
      getSession({ path: { session_id: sessionId } }).then((res) => {
        if (!cancelled && res.data?.goose_mode) {
          setGooseMode(res.data.goose_mode);
        }
      });
    } else {
      const mode = config.GOOSE_MODE as string | undefined;
      if (mode) {
        setGooseMode(mode);
      }
    }
    return () => {
      cancelled = true;
    };
  }, [sessionId, config.GOOSE_MODE]);

  const handleModeChange = async (newMode: string) => {
    if (gooseMode === newMode) {
      return;
    }

    try {
      if (sessionId) {
        await updateSession({ body: { session_id: sessionId, goose_mode: newMode } });
      }
      setGooseMode(newMode);
      trackModeChanged(gooseMode, newMode);
    } catch (error) {
      console.error('Error updating goose mode:', error);
      throw new Error(`Failed to store new goose mode: ${newMode}`);
    }
  };

  function getValueByKey(key: string): string {
    const mode = all_goose_modes.find((mode) => mode.key === key);
    if (!mode) return intl.formatMessage(i18n.autoFallback);
    return intl.formatMessage(mode.labelDescriptor);
  }

  function getModeDescription(key: string): string {
    const mode = all_goose_modes.find((mode) => mode.key === key);
    if (!mode) return intl.formatMessage(i18n.automaticModeDescription);
    return intl.formatMessage(mode.descriptionDescriptor);
  }

  return (
    <div title={intl.formatMessage(i18n.currentModeTitle, { label: getValueByKey(gooseMode), description: getModeDescription(gooseMode) })}>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <span className="flex items-center cursor-pointer [&_svg]:size-4 text-text-primary/70 hover:text-text-primary hover:scale-100 hover:bg-transparent text-xs">
            <Tornado className="mr-1 h-4 w-4" />
            {getValueByKey(gooseMode).toLowerCase()}
          </span>
        </DropdownMenuTrigger>
        <DropdownMenuContent className="w-64" side="top" align="center">
          {all_goose_modes.map((mode) => (
            <DropdownMenuItem key={mode.key} asChild>
              <ModeSelectionItem
                mode={mode}
                currentMode={gooseMode}
                showDescription={false}
                isApproveModeConfigure={false}
                handleModeChange={handleModeChange}
              />
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
};
