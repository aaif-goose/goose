import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Check, FolderDot, FolderOpen, GitBranch, Plus } from 'lucide-react';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '../ui/Tooltip';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '../ui/dropdown-menu';
import { updateWorkingDir } from '../../api';
import { toast } from 'react-toastify';
import { defineMessages, useIntl } from '../../i18n';

const i18n = defineMessages({
  failedToUpdateWorkingDir: {
    id: 'dirSwitcher.failedToUpdateWorkingDir',
    defaultMessage: 'Failed to update working directory',
  },
  currentDirectory: {
    id: 'dirSwitcher.currentDirectory',
    defaultMessage: 'Current directory',
  },
  gitWorktrees: {
    id: 'dirSwitcher.gitWorktrees',
    defaultMessage: 'Git worktrees',
  },
  recentDirectories: {
    id: 'dirSwitcher.recentDirectories',
    defaultMessage: 'Recent directories',
  },
  chooseDirectory: {
    id: 'dirSwitcher.chooseDirectory',
    defaultMessage: 'Choose directory…',
  },
  openInFinder: {
    id: 'dirSwitcher.openInFinder',
    defaultMessage: 'Open in file manager',
  },
  noWorktreesFound: {
    id: 'dirSwitcher.noWorktreesFound',
    defaultMessage: 'No worktrees found',
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
  const [isMenuOpen, setIsMenuOpen] = useState(false);
  const [recentDirs, setRecentDirs] = useState<string[]>([]);
  const [worktreeDirs, setWorktreeDirs] = useState<string[]>([]);
  const refreshVersionRef = useRef(0);

  const refreshMenuData = useCallback(async () => {
    const version = ++refreshVersionRef.current;
    setRecentDirs([]);
    setWorktreeDirs([]);

    const [recent, worktrees] = await Promise.all([
      window.electron.listRecentDirs().catch(() => []),
      window.electron.listGitWorktreeDirs(workingDir).catch(() => []),
    ]);

    if (version !== refreshVersionRef.current) return;

    setRecentDirs(recent);
    setWorktreeDirs(worktrees);
  }, [workingDir]);

  useEffect(() => {
    if (!isMenuOpen) {
      return;
    }

    void refreshMenuData();
  }, [isMenuOpen, refreshMenuData]);

  const applyDirectoryChange = async (newDir: string) => {
    window.electron.addRecentDir(newDir);
    setRecentDirs((previous) => [newDir, ...previous.filter((dir) => dir !== newDir)].slice(0, 10));

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
    await applyDirectoryChange(newDir);
  };

  const handleSelectDirectory = async (newDir: string) => {
    if (newDir === workingDir) {
      setIsMenuOpen(false);
      return;
    }

    setIsMenuOpen(false);
    await applyDirectoryChange(newDir);
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
    }
  };

  const filteredWorktreeDirs = useMemo(
    () => worktreeDirs.filter((dir) => dir && dir !== workingDir),
    [worktreeDirs, workingDir]
  );

  const filteredRecentDirs = useMemo(
    () => recentDirs.filter((dir) => dir && dir !== workingDir),
    [recentDirs, workingDir]
  );

  return (
    <TooltipProvider>
      <Tooltip
        open={isTooltipOpen && !isDirectoryChooserOpen && !isMenuOpen}
        onOpenChange={(open) => {
          if (!isDirectoryChooserOpen && !isMenuOpen) setIsTooltipOpen(open);
        }}
      >
        <DropdownMenu open={isMenuOpen} onOpenChange={setIsMenuOpen}>
          <TooltipTrigger asChild>
            <DropdownMenuTrigger asChild>
              <button
                className={`z-[100] ${isDirectoryChooserOpen ? 'opacity-50' : 'hover:cursor-pointer hover:text-text-primary'} text-text-primary/70 text-xs flex items-center transition-colors pl-1 [&>svg]:size-4 ${className}`}
                onClick={handleDirectoryClick}
                disabled={isDirectoryChooserOpen}
              >
                <FolderDot className="mr-1" size={16} />
                <div className="max-w-[200px] truncate [direction:rtl]">{workingDir}</div>
              </button>
            </DropdownMenuTrigger>
          </TooltipTrigger>
          <DropdownMenuContent className="w-80" side="top" align="start">
            <DropdownMenuLabel>{intl.formatMessage(i18n.currentDirectory)}</DropdownMenuLabel>
            <DropdownMenuItem
              onSelect={() => void window.electron.openDirectoryInExplorer(workingDir)}
            >
              <FolderOpen className="mr-2 h-4 w-4" />
              <span className="truncate">{workingDir}</span>
              <Check className="ml-auto h-4 w-4" />
            </DropdownMenuItem>

            <DropdownMenuSeparator />
            <DropdownMenuLabel>{intl.formatMessage(i18n.gitWorktrees)}</DropdownMenuLabel>
            {filteredWorktreeDirs.length > 0 ? (
              filteredWorktreeDirs.map((dir) => (
                <DropdownMenuItem
                  key={`worktree-${dir}`}
                  onSelect={() => void handleSelectDirectory(dir)}
                >
                  <GitBranch className="mr-2 h-4 w-4" />
                  <span className="truncate">{dir}</span>
                </DropdownMenuItem>
              ))
            ) : (
              <DropdownMenuItem disabled>
                <GitBranch className="mr-2 h-4 w-4" />
                <span>{intl.formatMessage(i18n.noWorktreesFound)}</span>
              </DropdownMenuItem>
            )}

            {filteredRecentDirs.length > 0 && (
              <>
                <DropdownMenuSeparator />
                <DropdownMenuLabel>{intl.formatMessage(i18n.recentDirectories)}</DropdownMenuLabel>
                {filteredRecentDirs.map((dir) => (
                  <DropdownMenuItem
                    key={`recent-${dir}`}
                    onSelect={() => void handleSelectDirectory(dir)}
                  >
                    <FolderDot className="mr-2 h-4 w-4" />
                    <span className="truncate">{dir}</span>
                  </DropdownMenuItem>
                ))}
              </>
            )}

            <DropdownMenuSeparator />
            <DropdownMenuItem onSelect={() => void handleDirectoryChange()}>
              <Plus className="mr-2 h-4 w-4" />
              <span>{intl.formatMessage(i18n.chooseDirectory)}</span>
            </DropdownMenuItem>
            <DropdownMenuItem
              onSelect={() => void window.electron.openDirectoryInExplorer(workingDir)}
            >
              <FolderOpen className="mr-2 h-4 w-4" />
              <span>{intl.formatMessage(i18n.openInFinder)}</span>
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
        <TooltipContent side="top">{workingDir}</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
};
