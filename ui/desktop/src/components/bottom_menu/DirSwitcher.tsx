import React, { useState } from 'react';
import { FolderDot } from 'lucide-react';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '../ui/Tooltip';
import { updateWorkingDir } from '../../api';
import { toast } from 'react-toastify';
import { defineMessages, useIntl } from '../../i18n';

const i18n = defineMessages({
  failedToUpdateWorkingDir: {
    id: 'dirSwitcher.failedToUpdateWorkingDir',
    defaultMessage: 'Failed to update working directory',
  },
});

interface DirSwitcherProps {
  className: string;
  sessionId: string | undefined;
  workingDir: string;
  onWorkingDirChange?: (newDir: string) => void;
  onRestartStart?: () => void;
  onRestartEnd?: () => void;
}

export const DirSwitcher: React.FC<DirSwitcherProps> = ({
  className,
  sessionId,
  workingDir,
  onWorkingDirChange,
  onRestartStart,
  onRestartEnd,
}) => {
  const intl = useIntl();
  const [isTooltipOpen, setIsTooltipOpen] = useState(false);
  const [isDirectoryChooserOpen, setIsDirectoryChooserOpen] = useState(false);

  const handleDirectoryChange = async () => {
    if (isDirectoryChooserOpen) return;
    setIsDirectoryChooserOpen(true);

    let result;
    try {
      result = await window.electron.directoryChooser();
    } finally {
      setIsDirectoryChooserOpen(false);
    }

    if (result.canceled || result.filePaths.length === 0) {
      return;
    }

    const newDir = result.filePaths[0];

    window.electron.addRecentDir(newDir);

    if (sessionId) {
      onWorkingDirChange?.(newDir);
      onRestartStart?.();

      try {
        await updateWorkingDir({
          body: { session_id: sessionId, working_dir: newDir },
        });
      } catch (error) {
        console.error('[DirSwitcher] Failed to update working directory:', error);
        toast.error(intl.formatMessage(i18n.failedToUpdateWorkingDir));
      } finally {
        onRestartEnd?.();
      }
    } else {
      onWorkingDirChange?.(newDir);
    }
  };

  const handleDirectoryClick = async (event: React.MouseEvent) => {
    if (isDirectoryChooserOpen) {
      event.preventDefault();
      event.stopPropagation();
      return;
    }
    const isCmdOrCtrlClick = event.metaKey || event.ctrlKey;

    if (isCmdOrCtrlClick) {
      event.preventDefault();
      event.stopPropagation();
      await window.electron.openDirectoryInExplorer(workingDir);
    } else {
      await handleDirectoryChange();
    }
  };

  return (
    <TooltipProvider>
      <Tooltip
        open={isTooltipOpen && !isDirectoryChooserOpen}
        onOpenChange={(open) => {
          if (!isDirectoryChooserOpen) setIsTooltipOpen(open);
        }}
      >
        <TooltipTrigger asChild>
          <button
            className={`z-[100] ${isDirectoryChooserOpen ? 'opacity-50' : 'hover:cursor-pointer hover:text-text-primary'} text-text-primary/70 text-xs flex items-center transition-colors pl-1 [&>svg]:size-4 ${className}`}
            onClick={handleDirectoryClick}
            disabled={isDirectoryChooserOpen}
          >
            <FolderDot className="mr-1" size={16} />
            <div className="max-w-[200px] truncate [direction:rtl]">{workingDir}</div>
          </button>
        </TooltipTrigger>
        <TooltipContent side="top">{workingDir}</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
};
