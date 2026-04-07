import React, { useState, useEffect } from 'react';
import { Button } from '../../ui/button';
import { Loader2, Download, CheckCircle, AlertCircle } from 'lucide-react';
import { errorMessage } from '../../../utils/conversionUtils';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  loading: {
    id: 'updateSection.loading',
    defaultMessage: 'Loading...',
  },
  currentVersion: {
    id: 'updateSection.currentVersion',
    defaultMessage: 'Current version',
  },
  versionAvailable: {
    id: 'updateSection.versionAvailable',
    defaultMessage: '→ {version} available',
  },
  upToDate: {
    id: 'updateSection.upToDate',
    defaultMessage: '(up to date)',
  },
  checkForUpdates: {
    id: 'updateSection.checkForUpdates',
    defaultMessage: 'Check for Updates',
  },
  installAndRestart: {
    id: 'updateSection.installAndRestart',
    defaultMessage: 'Install & Restart',
  },
  checking: {
    id: 'updateSection.checking',
    defaultMessage: 'Checking for updates...',
  },
  downloadingProgress: {
    id: 'updateSection.downloadingProgress',
    defaultMessage: 'Downloading update... {percent}%',
  },
  downloadReady: {
    id: 'updateSection.downloadReady',
    defaultMessage: 'Update downloaded and ready to install!',
  },
  latestVersion: {
    id: 'updateSection.latestVersion',
    defaultMessage: 'You are running the latest version!',
  },
  updateAvailable: {
    id: 'updateSection.updateAvailable',
    defaultMessage: 'Update available!',
  },
  versionIsAvailable: {
    id: 'updateSection.versionIsAvailable',
    defaultMessage: 'Version {version} is available',
  },
  downloadingUpdate: {
    id: 'updateSection.downloadingUpdate',
    defaultMessage: 'Downloading update...',
  },
  autoDownload: {
    id: 'updateSection.autoDownload',
    defaultMessage: 'Update will be downloaded automatically in the background.',
  },
  manualInstallNote: {
    id: 'updateSection.manualInstallNote',
    defaultMessage: "After download, you'll need to manually install the update.",
  },
  autoInstallNote: {
    id: 'updateSection.autoInstallNote',
    defaultMessage: 'The update will be installed automatically when you quit the app.',
  },
  readyInstallManual: {
    id: 'updateSection.readyInstallManual',
    defaultMessage: '✓ Update is ready! Click "Install & Restart" for installation instructions.',
  },
  manualInstallRequired: {
    id: 'updateSection.manualInstallRequired',
    defaultMessage: 'Manual installation required for this update method.',
  },
  readyInstallAuto: {
    id: 'updateSection.readyInstallAuto',
    defaultMessage: '✓ Update is ready! It will be installed when you quit Goose.',
  },
  installNowHint: {
    id: 'updateSection.installNowHint',
    defaultMessage: 'Or click "Install & Restart" to update now.',
  },
});

type UpdateStatus =
  | 'idle'
  | 'checking'
  | 'downloading'
  | 'installing'
  | 'success'
  | 'error'
  | 'ready';

interface UpdateInfo {
  currentVersion: string;
  latestVersion?: string;
  isUpdateAvailable?: boolean;
  error?: string;
}

interface UpdateEventData {
  version?: string;
  percent?: number;
}

export default function UpdateSection() {
  const intl = useIntl();
  const [updateStatus, setUpdateStatus] = useState<UpdateStatus>('idle');
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo>({
    currentVersion: '',
  });
  const [progress, setProgress] = useState<number>(0);
  const [isUsingGitHubFallback, setIsUsingGitHubFallback] = useState<boolean>(false);
  const progressTimeoutRef = React.useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastProgressRef = React.useRef<number>(0); // Track last progress to prevent backward jumps

  useEffect(() => {
    // Get current version on mount
    const currentVersion = window.electron.getVersion();
    setUpdateInfo((prev) => ({ ...prev, currentVersion }));

    // Check if there's already an update state from the auto-check
    window.electron.getUpdateState().then((state) => {
      if (state) {
        setUpdateInfo((prev) => ({
          ...prev,
          isUpdateAvailable: state.updateAvailable,
          latestVersion: state.latestVersion,
        }));
      }
    });

    // Check if using GitHub fallback
    window.electron.isUsingGitHubFallback().then((isGitHub) => {
      setIsUsingGitHubFallback(isGitHub);
    });

    // Listen for updater events
    window.electron.onUpdaterEvent((event) => {

      switch (event.event) {
        case 'checking-for-update':
          setUpdateStatus('checking');
          break;

        case 'update-available':
          setUpdateStatus('idle');
          setUpdateInfo((prev) => ({
            ...prev,
            latestVersion: (event.data as UpdateEventData)?.version,
            isUpdateAvailable: true,
          }));
          // Check if GitHub fallback is being used
          window.electron.isUsingGitHubFallback().then((isGitHub) => {
            setIsUsingGitHubFallback(isGitHub);
          });
          break;

        case 'update-not-available':
          setUpdateStatus('idle');
          setUpdateInfo((prev) => ({
            ...prev,
            isUpdateAvailable: false,
          }));
          break;

        case 'download-progress': {
          setUpdateStatus('downloading');

          // Get the new progress value (ensure it's a valid number)
          const rawPercent = (event.data as UpdateEventData)?.percent;
          const newProgress = typeof rawPercent === 'number' ? Math.round(rawPercent) : 0;

          // Only update if progress increased (prevents backward jumps from out-of-order events)
          if (newProgress > lastProgressRef.current) {
            lastProgressRef.current = newProgress;

            // Cancel any pending update
            if (progressTimeoutRef.current) {
              clearTimeout(progressTimeoutRef.current);
            }

            // Use a small delay to batch rapid updates
            progressTimeoutRef.current = setTimeout(() => {
              setProgress(newProgress);
            }, 50); // 50ms delay for smoother batching
          }
          break;
        }

        case 'update-downloaded':
          setUpdateStatus('ready');
          setProgress(100);
          break;

        case 'error':
          setUpdateStatus('error');
          setUpdateInfo((prev) => ({
            ...prev,
            error: String(event.data || 'An error occurred'),
          }));
          setTimeout(() => setUpdateStatus('idle'), 5000);
          break;
      }
    });

    return () => {
      if (progressTimeoutRef.current) {
        clearTimeout(progressTimeoutRef.current);
      }
    };
  }, []);

  const checkForUpdates = async () => {
    setUpdateStatus('checking');
    setProgress(0);
    lastProgressRef.current = 0; // Reset progress tracking for new download

    try {
      const result = await window.electron.checkForUpdates();

      if (result.error) {
        throw new Error(result.error);
      }

      // If we successfully checked and no update is available, show success
      if (!result.error && updateInfo.isUpdateAvailable === false) {
        setUpdateStatus('success');
        setTimeout(() => setUpdateStatus('idle'), 3000);
      }
      // The actual status will be handled by the updater events
    } catch (error) {
      console.error('Error checking for updates:', error);
      setUpdateInfo((prev) => ({
        ...prev,
        error: errorMessage(error, 'Failed to check for updates'),
      }));
      setUpdateStatus('error');
      setTimeout(() => setUpdateStatus('idle'), 5000);
    }
  };

  const installUpdate = () => {
    window.electron.installUpdate();
  };

  const getStatusMessage = () => {
    switch (updateStatus) {
      case 'checking':
        return intl.formatMessage(i18n.checking);
      case 'downloading':
        return intl.formatMessage(i18n.downloadingProgress, { percent: Math.round(progress) });
      case 'ready':
        return intl.formatMessage(i18n.downloadReady);
      case 'success':
        return updateInfo.isUpdateAvailable === false
          ? intl.formatMessage(i18n.latestVersion)
          : intl.formatMessage(i18n.updateAvailable);
      case 'error':
        return updateInfo.error || 'An error occurred';
      default:
        if (updateInfo.isUpdateAvailable) {
          return intl.formatMessage(i18n.versionIsAvailable, { version: updateInfo.latestVersion });
        }
        return '';
    }
  };

  const getStatusIcon = () => {
    switch (updateStatus) {
      case 'checking':
      case 'downloading':
        return <Loader2 className="w-4 h-4 animate-spin" />;
      case 'success':
        return <CheckCircle className="w-4 h-4 text-green-500" />;
      case 'error':
        return <AlertCircle className="w-4 h-4 text-red-500" />;
      case 'ready':
        return <CheckCircle className="w-4 h-4 text-blue-500" />;
      default:
        return updateInfo.isUpdateAvailable ? <Download className="w-4 h-4" /> : null;
    }
  };

  return (
    <div>
      <div className="text-sm text-text-secondary mb-4 flex items-center gap-2">
        <div className="flex flex-col">
          <div className="text-text-primary text-2xl font-mono">
            {updateInfo.currentVersion || intl.formatMessage(i18n.loading)}
          </div>
          <div className="text-xs text-text-secondary">{intl.formatMessage(i18n.currentVersion)}</div>
        </div>
        {updateInfo.latestVersion && updateInfo.isUpdateAvailable && (
          <span className="text-text-secondary"> {intl.formatMessage(i18n.versionAvailable, { version: updateInfo.latestVersion })}</span>
        )}
        {updateInfo.currentVersion && updateInfo.isUpdateAvailable === false && (
          <span className="text-text-primary"> {intl.formatMessage(i18n.upToDate)}</span>
        )}
      </div>

      <div className="flex gap-2">
        <div className="flex items-center gap-2">
          <Button
            onClick={checkForUpdates}
            disabled={updateStatus !== 'idle' && updateStatus !== 'error'}
            variant="secondary"
            size="sm"
          >
            {intl.formatMessage(i18n.checkForUpdates)}
          </Button>

          {updateStatus === 'ready' && (
            <Button onClick={installUpdate} variant="default" size="sm">
              {intl.formatMessage(i18n.installAndRestart)}
            </Button>
          )}
        </div>

        {getStatusMessage() && (
          <div className="flex items-center gap-2 text-xs text-text-secondary">
            {getStatusIcon()}
            <span>{getStatusMessage()}</span>
          </div>
        )}

        {updateStatus === 'downloading' && (
          <div className="w-full mt-2">
            <div className="flex justify-between text-xs text-text-secondary mb-1">
              <span>{intl.formatMessage(i18n.downloadingUpdate)}</span>
              <span>{progress}%</span>
            </div>
            <div className="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2 overflow-hidden">
              <div
                className="bg-blue-500 h-2 rounded-full transition-[width] duration-150 ease-out"
                style={{ width: `${Math.max(progress, 0)}%`, minWidth: progress > 0 ? '8px' : '0' }}
              />
            </div>
          </div>
        )}

        {/* Update information */}
        {updateInfo.isUpdateAvailable && updateStatus === 'idle' && (
          <div className="text-xs text-text-secondary mt-4 space-y-1">
            <p>{intl.formatMessage(i18n.autoDownload)}</p>
            {isUsingGitHubFallback ? (
              <p className="text-xs text-amber-600">
                {intl.formatMessage(i18n.manualInstallNote)}
              </p>
            ) : (
              <p className="text-xs text-green-600">
                {intl.formatMessage(i18n.autoInstallNote)}
              </p>
            )}
          </div>
        )}

        {updateStatus === 'ready' && (
          <div className="text-xs text-text-secondary mt-4 space-y-1">
            {isUsingGitHubFallback ? (
              <>
                <p className="text-xs text-green-600">
                  {intl.formatMessage(i18n.readyInstallManual)}
                </p>
                <p className="text-xs text-text-secondary">
                  {intl.formatMessage(i18n.manualInstallRequired)}
                </p>
              </>
            ) : (
              <>
                <p className="text-xs text-green-600">
                  {intl.formatMessage(i18n.readyInstallAuto)}
                </p>
                <p className="text-xs text-text-secondary">
                  {intl.formatMessage(i18n.installNowHint)}
                </p>
              </>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
