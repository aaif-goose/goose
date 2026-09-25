import { useEffect, useMemo, useState, type ReactNode } from 'react';
import { Layers, Plus, RefreshCw, Trash2 } from 'lucide-react';
import { Button } from '../ui/button';
import { Switch } from '../ui/switch';
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from '../ui/card';
import { useClientExtensions } from '../../client-extensions/ClientExtensionsContext';
import type {
  ClientExtensionManifest,
  DiscoveredClientExtension,
} from '../../client-extensions/types';
import { defineMessages, useIntl } from '../../i18n';
import { cn } from '../../utils';
import { toastService } from '../../toasts';

function useClientExtensionsInstallDir(): string | null {
  return useClientExtensions().installDir;
}

const i18n = defineMessages({
  emptyTitle: {
    id: 'pluginsView.emptyTitle',
    defaultMessage: 'No plugins installed yet',
  },
  empty: {
    id: 'pluginsView.empty',
    defaultMessage:
      'Use Install plugin to pick a folder with client-extension.json, or enable dev examples from your local checkout.',
  },
  installHint: {
    id: 'pluginsView.installHint',
    defaultMessage: 'Install directory:',
  },
  reload: {
    id: 'pluginsView.reload',
    defaultMessage: 'Reload',
  },
  install: {
    id: 'pluginsView.install',
    defaultMessage: 'Install plugin',
  },
  installSuccess: {
    id: 'pluginsView.installSuccess',
    defaultMessage: 'Installed plugin',
  },
  installFailed: {
    id: 'pluginsView.installFailed',
    defaultMessage: 'Failed to install plugin',
  },
  uninstallSuccess: {
    id: 'pluginsView.uninstallSuccess',
    defaultMessage: 'Uninstalled plugin',
  },
  uninstallFailed: {
    id: 'pluginsView.uninstallFailed',
    defaultMessage: 'Failed to uninstall plugin',
  },
  confirmUninstall: {
    id: 'pluginsView.confirmUninstall',
    defaultMessage: 'Uninstall "{name}"? This removes it from your install directory.',
  },
  devUninstallHint: {
    id: 'pluginsView.devUninstallHint',
    defaultMessage: 'Dev examples live in your repo — disable them here instead of uninstalling.',
  },
  version: {
    id: 'pluginsView.version',
    defaultMessage: 'Version {version}',
  },
  devSource: {
    id: 'pluginsView.devSource',
    defaultMessage: 'Dev example',
  },
  installedSource: {
    id: 'pluginsView.installedSource',
    defaultMessage: 'Installed',
  },
  disabledSource: {
    id: 'pluginsView.disabledSource',
    defaultMessage: 'Disabled',
  },
  noContributions: {
    id: 'pluginsView.noContributions',
    defaultMessage: 'No UI contributions declared',
  },
  contributionPage: {
    id: 'pluginsView.contribution.page',
    defaultMessage: 'page',
  },
  contributionChatAction: {
    id: 'pluginsView.contribution.chatAction',
    defaultMessage: 'chat action',
  },
  contributionMessageSuffix: {
    id: 'pluginsView.contribution.messageSuffix',
    defaultMessage: 'message decoration',
  },
  contributionCustomRender: {
    id: 'pluginsView.contribution.customRender',
    defaultMessage: 'custom render',
  },
  contributionSidecar: {
    id: 'pluginsView.contribution.sidecar',
    defaultMessage: 'side panel',
  },
  togglePlugin: {
    id: 'pluginsView.togglePlugin',
    defaultMessage: 'Toggle {name}',
  },
  uninstall: {
    id: 'pluginsView.uninstall',
    defaultMessage: 'Uninstall',
  },
  uninstallPlugin: {
    id: 'pluginsView.uninstallPlugin',
    defaultMessage: 'Uninstall {name}',
  },
});

function contributionTags(manifest: ClientExtensionManifest, intl: ReturnType<typeof useIntl>) {
  const tags: string[] = [];
  for (const link of manifest.contributes?.rootLinks ?? []) {
    tags.push(`${intl.formatMessage(i18n.contributionPage)}: ${link.id}`);
  }
  for (const action of manifest.contributes?.chatActions ?? []) {
    tags.push(`${intl.formatMessage(i18n.contributionChatAction)}: ${action.id}`);
  }
  for (const suffix of manifest.contributes?.contentSuffixes ?? []) {
    tags.push(`${intl.formatMessage(i18n.contributionMessageSuffix)}: ${suffix.id}`);
  }
  for (const render of manifest.contributes?.customRenders ?? []) {
    tags.push(`${intl.formatMessage(i18n.contributionCustomRender)}: ${render.id}`);
  }
  for (const sidecar of manifest.contributes?.sidecars ?? []) {
    tags.push(`${intl.formatMessage(i18n.contributionSidecar)}: ${sidecar.id}`);
  }
  return tags;
}

function PluginCard({
  extension,
  loading,
  onToggle,
  onUninstall,
}: {
  extension: DiscoveredClientExtension;
  loading: boolean;
  onToggle: (enabled: boolean) => Promise<void>;
  onUninstall?: (extensionId: string) => Promise<void>;
}) {
  const intl = useIntl();
  const [visuallyEnabled, setVisuallyEnabled] = useState(extension.enabled);
  const [isToggling, setIsToggling] = useState(false);
  const [isUninstalling, setIsUninstalling] = useState(false);

  useEffect(() => {
    if (!isToggling) {
      setVisuallyEnabled(extension.enabled);
    }
  }, [extension.enabled, isToggling]);

  const tags = useMemo(
    () => contributionTags(extension.manifest, intl),
    [extension.manifest, intl]
  );

  const handleToggle = async () => {
    if (isToggling || loading) {
      return;
    }

    const nextState = !visuallyEnabled;
    setIsToggling(true);
    setVisuallyEnabled(nextState);
    try {
      await onToggle(nextState);
    } catch {
      setVisuallyEnabled(!nextState);
    } finally {
      setIsToggling(false);
    }
  };

  const handleUninstall = async () => {
    if (!onUninstall || isUninstalling || loading) {
      return;
    }

    if (!window.confirm(intl.formatMessage(i18n.confirmUninstall, { name: extension.id }))) {
      return;
    }

    setIsUninstalling(true);
    try {
      await onUninstall(extension.id);
      toastService.success({
        title: intl.formatMessage(i18n.uninstallSuccess),
        msg: extension.id,
      });
    } catch (error) {
      toastService.error({
        title: intl.formatMessage(i18n.uninstallFailed),
        msg: error instanceof Error ? error.message : extension.id,
      });
    } finally {
      setIsUninstalling(false);
    }
  };

  const canUninstall = extension.source === 'installed' && onUninstall;

  return (
    <Card
      id={`plugin-${extension.id}`}
      className={cn(
        'min-h-[160px] transition-all duration-200 hover:border-border-primary',
        !extension.enabled && 'opacity-75'
      )}
    >
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <Layers className="h-4 w-4 text-text-secondary" />
          <span className="truncate">{extension.id}</span>
        </CardTitle>
        <CardAction>
          <div className="flex items-center gap-2">
            {canUninstall && (
              <Button
                type="button"
                variant="outline"
                size="xs"
                disabled={loading || isUninstalling}
                onClick={() => void handleUninstall()}
                className="text-text-secondary hover:text-destructive hover:border-destructive"
                aria-label={intl.formatMessage(i18n.uninstallPlugin, { name: extension.id })}
              >
                <Trash2 className="h-3.5 w-3.5" />
                {intl.formatMessage(i18n.uninstall)}
              </Button>
            )}
            <Switch
              checked={visuallyEnabled}
              onCheckedChange={() => void handleToggle()}
              disabled={loading || isToggling}
              variant="mono"
              aria-label={intl.formatMessage(i18n.togglePlugin, { name: extension.id })}
            />
          </div>
        </CardAction>
        <CardDescription className="flex flex-wrap items-center gap-2 pt-1">
          <span>{intl.formatMessage(i18n.version, { version: extension.manifest.version })}</span>
          <span className="inline-block rounded bg-background-secondary px-2 py-0.5 text-xs">
            {extension.source === 'dev'
              ? intl.formatMessage(i18n.devSource)
              : intl.formatMessage(i18n.installedSource)}
          </span>
          {!extension.enabled && (
            <span className="inline-block rounded bg-background-secondary px-2 py-0.5 text-xs">
              {intl.formatMessage(i18n.disabledSource)}
            </span>
          )}
        </CardDescription>
      </CardHeader>
      <CardContent className="px-4 pt-0">
        {tags.length === 0 ? (
          <p className="text-sm text-text-secondary">{intl.formatMessage(i18n.noContributions)}</p>
        ) : (
          <div className="flex flex-wrap gap-2">
            {tags.map((tag) => (
              <span
                key={tag}
                className="inline-block rounded bg-background-secondary px-2 py-1 text-xs text-text-secondary"
              >
                {tag}
              </span>
            ))}
          </div>
        )}
        {extension.source === 'dev' && (
          <p className="mt-3 text-xs text-text-secondary">
            {intl.formatMessage(i18n.devUninstallHint)}
          </p>
        )}
      </CardContent>
    </Card>
  );
}

function PluginsGrid({ children }: { children: ReactNode }) {
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
}

export function PluginsInstallButton({
  loading,
  onInstall,
}: {
  loading: boolean;
  onInstall: () => void;
}) {
  const intl = useIntl();

  return (
    <Button
      type="button"
      variant="default"
      size="sm"
      onClick={onInstall}
      disabled={loading}
      className="flex items-center gap-2"
    >
      <Plus className="h-4 w-4" />
      {intl.formatMessage(i18n.install)}
    </Button>
  );
}

export function useInstallPluginFromFolder() {
  const intl = useIntl();
  const { installExtension, loading } = useClientExtensions();

  return {
    loading,
    installFromFolder: async () => {
      const result = await window.electron.directoryChooser();
      const sourcePath = result.canceled ? null : result.filePaths[0];
      if (!sourcePath) {
        return;
      }

      try {
        await installExtension(sourcePath);
        toastService.success({
          title: intl.formatMessage(i18n.installSuccess),
          msg: sourcePath,
        });
      } catch (error) {
        toastService.error({
          title: intl.formatMessage(i18n.installFailed),
          msg: error instanceof Error ? error.message : sourcePath,
        });
      }
    },
  };
}

export function PluginsPanel() {
  const intl = useIntl();
  const { extensions, loading, setExtensionEnabled, uninstallExtension } = useClientExtensions();
  const installDir = useClientExtensionsInstallDir();

  if (extensions.length === 0) {
    return (
      <div className="flex min-h-[320px] flex-col items-center justify-center gap-4">
        <div className="max-w-md text-center">
          <h3 className="mb-2 text-lg font-medium">{intl.formatMessage(i18n.emptyTitle)}</h3>
          <p className="text-sm text-text-secondary">{intl.formatMessage(i18n.empty)}</p>
          {installDir && (
            <p className="mt-4 break-all font-mono text-xs text-text-secondary">
              {intl.formatMessage(i18n.installHint)} {installDir}
            </p>
          )}
        </div>
      </div>
    );
  }

  return (
    <PluginsGrid>
      {extensions.map((extension) => (
        <PluginCard
          key={extension.id}
          extension={extension}
          loading={loading}
          onToggle={(enabled) => setExtensionEnabled(extension.id, enabled)}
          onUninstall={uninstallExtension}
        />
      ))}
    </PluginsGrid>
  );
}

export function PluginsInstallHint() {
  const intl = useIntl();
  const installDir = useClientExtensionsInstallDir();

  if (!installDir) {
    return null;
  }

  return (
    <p className="mb-6 font-mono text-xs text-text-secondary">
      {intl.formatMessage(i18n.installHint)} {installDir}
    </p>
  );
}

export function PluginsReloadButton({
  loading,
  onReload,
}: {
  loading: boolean;
  onReload: () => void;
}) {
  const intl = useIntl();

  return (
    <Button
      type="button"
      variant="outline"
      size="sm"
      onClick={onReload}
      disabled={loading}
      className="flex items-center gap-2"
    >
      <RefreshCw className="h-4 w-4" />
      {intl.formatMessage(i18n.reload)}
    </Button>
  );
}
