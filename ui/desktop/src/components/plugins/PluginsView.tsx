import { MainPanelLayout } from '../Layout/MainPanelLayout';
import {
  PluginsInstallButton,
  PluginsInstallHint,
  PluginsPanel,
  PluginsReloadButton,
  useInstallPluginFromFolder,
} from './PluginsPanel';
import { useClientExtensions } from '../../client-extensions/ClientExtensionsContext';
import { defineMessages, useIntl } from '../../i18n';

const i18n = defineMessages({
  title: {
    id: 'pluginsView.title',
    defaultMessage: 'Plugins',
  },
  description: {
    id: 'pluginsView.description',
    defaultMessage:
      'Install UI plugins that extend goose Desktop with custom pages, chat actions, side panels, and message decorations. Distinct from MCP Extensions, which connect goose to external tools.',
  },
});

export default function PluginsView() {
  const intl = useIntl();
  const { loading, reloadExtensions } = useClientExtensions();
  const { installFromFolder } = useInstallPluginFromFolder();

  return (
    <MainPanelLayout>
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">
        <div className="bg-background-primary px-8 pb-4 pt-16">
          <div className="page-transition flex flex-col">
            <div className="mb-1 flex items-center justify-between gap-3">
              <h1 className="text-4xl font-light">{intl.formatMessage(i18n.title)}</h1>
              <div className="flex items-center gap-2">
                <PluginsInstallButton
                  loading={loading}
                  onInstall={() => void installFromFolder()}
                />
                <PluginsReloadButton loading={loading} onReload={() => void reloadExtensions()} />
              </div>
            </div>
            <p className="mb-2 max-w-3xl text-sm text-text-secondary">
              {intl.formatMessage(i18n.description)}
            </p>
            <PluginsInstallHint />
          </div>
        </div>

        <div className="flex-1 px-8 pb-16">
          <PluginsPanel />
        </div>
      </div>
    </MainPanelLayout>
  );
}
