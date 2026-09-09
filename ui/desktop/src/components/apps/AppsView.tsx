import { useCallback, useEffect, useRef, useState } from 'react';
import { MainPanelLayout } from '../Layout/MainPanelLayout';
import { Button } from '../ui/button';
import { AlertTriangle, Download, Play, Trash2, Upload } from 'lucide-react';
import type { GooseApp } from '../../types/apps';
import { deleteMcpApp, exportMcpApp, importMcpApp, listMcpApps } from '../../acp/mcp-apps';
import { useChatContext } from '../../contexts/ChatContext';
import { formatAppName } from '../../utils/conversionUtils';
import { errorMessage } from '../../utils/conversionUtils';
import { isRetiredGooseChatApp } from '../../utils/retiredApps';
import { defineMessages, useIntl } from '../../i18n';

const i18n = defineMessages({
  errorLoading: {
    id: 'appsView.errorLoading',
    defaultMessage: 'Error loading apps: {error}',
  },
  retry: {
    id: 'appsView.retry',
    defaultMessage: 'Retry',
  },
  title: {
    id: 'appsView.title',
    defaultMessage: 'Apps',
  },
  importApp: {
    id: 'appsView.importApp',
    defaultMessage: 'Import App',
  },
  description: {
    id: 'appsView.description',
    defaultMessage:
      'Applications from your MCP servers and Apps built by goose itself. You can ask it to create new apps through the chat interface and they will appear here.',
  },
  loading: {
    id: 'appsView.loading',
    defaultMessage: 'Loading apps...',
  },
  noAppsTitle: {
    id: 'appsView.noAppsTitle',
    defaultMessage: 'No apps available',
  },
  noAppsDescription: {
    id: 'appsView.noAppsDescription',
    defaultMessage:
      'Open a chat and ask goose for the app you want to have. It can build one for you and that will appear here. Or if somebody shared an app, you can import it using the button above.',
  },
  customApp: {
    id: 'appsView.customApp',
    defaultMessage: 'Custom app',
  },
  launch: {
    id: 'appsView.launch',
    defaultMessage: 'Launch',
  },
  retiredChatApp: {
    id: 'appsView.retiredChatApp',
    defaultMessage: 'Chat app retired',
  },
  retiredChatAppDetail: {
    id: 'appsView.retiredChatAppDetail',
    defaultMessage: 'We removed this feature because MCP sampling is no longer supported.',
  },
  deleteConfirm: {
    id: 'appsView.deleteConfirm',
    defaultMessage: 'Delete "{name}"? This cannot be undone.',
  },
  deleteApp: {
    id: 'appsView.deleteApp',
    defaultMessage: 'Delete app',
  },
  errorPrefix: {
    id: 'appsView.errorPrefix',
    defaultMessage: 'Error: {error}',
  },
});

const GridLayout = ({ children }: { children: React.ReactNode }) => {
  return (
    <div
      className="grid gap-4 p-1"
      style={{
        gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))',
        justifyContent: 'center',
      }}
    >
      {children}
    </div>
  );
};

export default function AppsView() {
  const intl = useIntl();
  const [apps, setApps] = useState<GooseApp[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [deletesInProgress, setDeletesInProgress] = useState<Set<string>>(new Set());
  const chatContext = useChatContext();
  const sessionId = chatContext?.chat.sessionId;

  // Load cached apps immediately on mount
  useEffect(() => {
    const loadCachedApps = async () => {
      try {
        const cachedApps = await listMcpApps();
        // Only show apps from the "apps" extension (vibe coded apps built by Goose)
        setApps(cachedApps.filter((a) => a.mcpServers?.includes('apps')));
      } catch (err) {
        console.warn('Failed to load cached apps:', err);
      } finally {
        setLoading(false);
      }
    };

    loadCachedApps();
  }, []);

  const refreshAppsExtensionList = useCallback(async (activeSessionId?: string) => {
    const appsExtension = 'apps';
    const cacheApps = (await listMcpApps()).filter((a) => a.mcpServers?.includes(appsExtension));

    if (!activeSessionId) {
      setApps(cacheApps);
      return;
    }

    try {
      const sessionApps = (await listMcpApps(activeSessionId)).filter((a) =>
        a.mcpServers?.includes(appsExtension)
      );
      const merged = new Map<string, GooseApp>();
      for (const app of cacheApps) {
        merged.set(app.uri, app);
      }
      for (const app of sessionApps) {
        merged.set(app.uri, app);
      }
      setApps(Array.from(merged.values()));
      setError(null);
    } catch (err) {
      console.warn('Failed to refresh apps from session:', err);
      setApps(cacheApps);
      if (cacheApps.length === 0) {
        setError(errorMessage(err, 'Failed to load apps'));
      }
    }
  }, []);

  useEffect(() => {
    if (!sessionId) return;

    refreshAppsExtensionList(sessionId);
  }, [sessionId, refreshAppsExtensionList]);

  useEffect(() => {
    const handlePlatformEvent = (event: Event) => {
      const customEvent = event as CustomEvent;
      const eventData = customEvent.detail;

      if (eventData?.extension === 'apps') {
        const eventSessionId = eventData.sessionId || sessionId;
        refreshAppsExtensionList(eventSessionId).catch((err) => {
          console.warn('Failed to refresh apps after platform event:', err);
        });
      }
    };

    window.addEventListener('platform-event', handlePlatformEvent);
    return () => window.removeEventListener('platform-event', handlePlatformEvent);
  }, [sessionId, refreshAppsExtensionList]);

  const loadApps = useCallback(async () => {
    if (!sessionId) return;

    try {
      setLoading(true);
      await refreshAppsExtensionList(sessionId);
    } finally {
      setLoading(false);
    }
  }, [sessionId, refreshAppsExtensionList]);

  const handleLaunchApp = async (app: GooseApp) => {
    try {
      await window.electron.launchApp(app);
    } catch (err) {
      console.error('Failed to launch app:', err);
      // App launch errors shouldn't hide the apps list, just log it
    }
  };

  const handleDeleteApp = async (app: GooseApp) => {
    if (
      !window.confirm(
        intl.formatMessage(i18n.deleteConfirm, { name: formatAppName(app.name) })
      )
    ) {
      return;
    }

    setDeletesInProgress((prev) => new Set(prev).add(app.name));
    setError(null);

    try {
      await deleteMcpApp(app.name);
      await window.electron.closeApp(app.name).catch(() => undefined);
      setApps((prev) => prev.filter((a) => a.name !== app.name));
    } catch (err) {
      console.error('Failed to delete app:', err);
      setError(errorMessage(err, 'Failed to delete app'));
    } finally {
      setDeletesInProgress((prev) => {
        const next = new Set(prev);
        next.delete(app.name);
        return next;
      });
    }
  };

  const handleDownloadApp = async (app: GooseApp) => {
    try {
      const html = await exportMcpApp(app.name);
      const blob = new Blob([html], { type: 'text/html' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `${app.name}.html`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
    } catch (err) {
      console.error('Failed to export app:', err);
      setError(errorMessage(err, 'Failed to export app'));
    }
  };

  const fileInputRef = useRef<HTMLInputElement>(null);

  const handleImportClick = () => {
    fileInputRef.current?.click();
  };

  const handleUploadApp = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;

    try {
      const text = await file.text();
      await importMcpApp(text);

      const cachedApps = await listMcpApps();
      // Only show apps from the "apps" extension (vibe coded apps built by Goose)
      setApps(cachedApps.filter((a) => a.mcpServers?.includes('apps')));
      setError(null);
    } catch (err) {
      console.error('Failed to import app:', err);
      setError(errorMessage(err, 'Failed to import app'));
    }
    event.target.value = '';
  };

  // Only show error-only UI if we have no apps to display
  if (error && apps.length === 0) {
    return (
      <MainPanelLayout>
        <div className="flex flex-col items-center justify-center h-64 text-center">
          <p className="text-red-500 mb-4">{intl.formatMessage(i18n.errorLoading, { error })}</p>
          <Button onClick={loadApps}>{intl.formatMessage(i18n.retry)}</Button>
        </div>
      </MainPanelLayout>
    );
  }

  return (
    <MainPanelLayout>
      <div className="flex-1 flex flex-col min-h-0">
        <input
          ref={fileInputRef}
          type="file"
          accept=".html"
          onChange={handleUploadApp}
          style={{ display: 'none' }}
        />
        <div className="bg-background-primary px-8 pb-8 pt-16">
          <div className="flex flex-col page-transition">
            <div className="flex justify-between items-center mb-1">
              <h1 className="text-4xl font-light">{intl.formatMessage(i18n.title)}</h1>
              <Button
                variant="outline"
                size="sm"
                onClick={handleImportClick}
                className="flex items-center gap-2"
              >
                <Upload className="h-4 w-4" />
                {intl.formatMessage(i18n.importApp)}
              </Button>
            </div>
            <div className="mb-4">
              <p className="text-sm text-text-secondary mb-2">
                {intl.formatMessage(i18n.description)}
              </p>
            </div>
          </div>
        </div>

        <div className="flex-1 overflow-y-auto px-8 pb-8">
          {error && apps.length > 0 && (
            <div className="mb-4">
              <p className="text-text-danger text-sm">
                {intl.formatMessage(i18n.errorPrefix, { error })}
              </p>
            </div>
          )}
          {loading ? (
            <div className="flex items-center justify-center h-64">
              <p className="text-text-secondary">{intl.formatMessage(i18n.loading)}</p>
            </div>
          ) : apps.length === 0 ? (
            <div className="flex items-center justify-center h-64">
              <div className="text-center">
                <h3 className="text-lg font-medium mb-2">{intl.formatMessage(i18n.noAppsTitle)}</h3>
                <p className="text-sm text-text-secondary">
                  {intl.formatMessage(i18n.noAppsDescription)}
                </p>
              </div>
            </div>
          ) : (
            <GridLayout>
              {apps.map((app) => {
                const isCustomApp = app.mcpServers?.includes('apps') ?? false;
                const retiredChatApp = isRetiredGooseChatApp(app);
                const canDelete = isCustomApp && app.deletable === true;
                const deleteInProgress = deletesInProgress.has(app.name);
                return (
                  <div
                    key={`${app.uri}-${app.mcpServers?.join(',')}`}
                    className="flex flex-col p-4 border rounded-lg hover:border-border-primary transition-colors"
                  >
                    {retiredChatApp && (
                      <div className="mb-3 flex items-start gap-2 rounded border border-amber-300 bg-amber-50 px-3 py-2 text-sm text-amber-900 dark:border-amber-700 dark:bg-amber-950 dark:text-amber-100">
                        <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
                        <div>
                          <div className="font-medium">
                            {intl.formatMessage(i18n.retiredChatApp)}
                          </div>
                          <p className="mt-1 text-xs leading-5">
                            {intl.formatMessage(i18n.retiredChatAppDetail)}
                          </p>
                        </div>
                      </div>
                    )}
                    <div className="flex-1 mb-4">
                      <h3 className="font-medium text-text-primary mb-2">
                        {formatAppName(app.name)}
                      </h3>
                      {app.description && (
                        <p className="text-sm text-text-secondary mb-2">{app.description}</p>
                      )}
                      {app.mcpServers && app.mcpServers.length > 0 && (
                        <span className="inline-block px-2 py-1 text-xs bg-background-secondary text-text-secondary rounded">
                          {isCustomApp
                            ? intl.formatMessage(i18n.customApp)
                            : app.mcpServers.join(', ')}
                        </span>
                      )}
                    </div>
                    <div className="flex gap-2">
                      <Button
                        variant="default"
                        size="sm"
                        onClick={() => handleLaunchApp(app)}
                        disabled={retiredChatApp}
                        className="flex items-center gap-2 flex-1"
                      >
                        <Play className="h-4 w-4" />
                        {intl.formatMessage(i18n.launch)}
                      </Button>
                      {isCustomApp && (
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={() => handleDownloadApp(app)}
                          className="flex items-center gap-2"
                        >
                          <Download className="h-4 w-4" />
                        </Button>
                      )}
                      {canDelete && (
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={() => handleDeleteApp(app)}
                          disabled={deleteInProgress}
                          className="flex items-center gap-2 text-destructive hover:text-destructive"
                          aria-label={intl.formatMessage(i18n.deleteApp)}
                        >
                          <Trash2 className="h-4 w-4" />
                        </Button>
                      )}
                    </div>
                  </div>
                );
              })}
            </GridLayout>
          )}
        </div>
      </div>
    </MainPanelLayout>
  );
}
