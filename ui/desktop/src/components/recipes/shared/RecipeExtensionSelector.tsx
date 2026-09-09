import { useState } from 'react';
import type { RecipeExtension } from '../../../recipe';
import { useConfig, type FixedExtensionEntry } from '../../ConfigContext';
import { Input } from '../../ui/input';
import { Switch } from '../../ui/switch';
import { formatExtensionName } from '../../settings/extensions/subcomponents/ExtensionList';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  label: {
    id: 'recipeExtensionSelector.label',
    defaultMessage: 'Extensions (Optional)',
  },
  description: {
    id: 'recipeExtensionSelector.description',
    defaultMessage:
      'Select which extensions should be available when running this recipe. Leave empty to use default extensions.',
  },
  searchPlaceholder: {
    id: 'recipeExtensionSelector.searchPlaceholder',
    defaultMessage: 'Search extensions...',
  },
  extensionsSelected: {
    id: 'recipeExtensionSelector.extensionsSelected',
    defaultMessage: '{count, plural, one {# extension} other {# extensions}} selected',
  },
  noExtensionsFound: {
    id: 'recipeExtensionSelector.noExtensionsFound',
    defaultMessage: 'No extensions found',
  },
  noExtensionsAvailable: {
    id: 'recipeExtensionSelector.noExtensionsAvailable',
    defaultMessage: 'No extensions available',
  },
});

type DisplayRecipeExtension = RecipeExtension & {
  enabled?: boolean;
};

function availableToolsProps(availableTools?: string[] | null) {
  return availableTools && availableTools.length > 0
    ? { available_tools: availableTools }
    : undefined;
}

function toRecipeExtension(
  extension: FixedExtensionEntry | DisplayRecipeExtension
): DisplayRecipeExtension | null {
  const enabled = 'enabled' in extension ? extension.enabled : undefined;

  switch (extension.type) {
    case 'builtin': {
      const { name, description, display_name, timeout, bundled, available_tools, type } =
        extension;
      return {
        name,
        description,
        display_name,
        timeout,
        bundled,
        ...availableToolsProps(available_tools),
        type,
        enabled,
      };
    }
    case 'platform': {
      const { name, description, display_name, bundled, available_tools, type } = extension;
      return {
        name,
        description,
        display_name,
        bundled,
        ...availableToolsProps(available_tools),
        type,
        enabled,
      };
    }
    case 'stdio': {
      const {
        name,
        description,
        cmd,
        args,
        envs,
        env_keys,
        timeout,
        cwd,
        bundled,
        available_tools,
        type,
      } = extension;
      return {
        name,
        description,
        cmd,
        args,
        envs,
        env_keys,
        timeout,
        cwd,
        bundled,
        ...availableToolsProps(available_tools),
        type,
        enabled,
      };
    }
    case 'streamable_http': {
      const {
        name,
        description,
        uri,
        envs,
        env_keys,
        headers,
        timeout,
        socket,
        client_id,
        client_secret_key,
        scopes,
        bundled,
        available_tools,
        type,
      } = extension;
      return {
        name,
        description,
        uri,
        envs,
        env_keys,
        headers,
        timeout,
        socket,
        client_id,
        client_secret_key,
        scopes,
        bundled,
        ...availableToolsProps(available_tools),
        type,
        enabled,
      };
    }
    default:
      return null;
  }
}

function removeDisplayFields(extension: DisplayRecipeExtension): RecipeExtension {
  const { enabled: _enabled, ...recipeExtension } = extension;
  return recipeExtension;
}

interface RecipeExtensionSelectorProps {
  selectedExtensions: RecipeExtension[];
  onExtensionsChange: (extensions: RecipeExtension[]) => void;
}

export const RecipeExtensionSelector = ({
  selectedExtensions,
  onExtensionsChange,
}: RecipeExtensionSelectorProps) => {
  const intl = useIntl();
  const { extensionsList: allExtensions } = useConfig();
  const [searchQuery, setSearchQuery] = useState('');

  const selectedExtensionNames = new Set(selectedExtensions.map((ext) => ext.name));

  const extensionMap = new Map<string, DisplayRecipeExtension>();
  allExtensions.forEach((extension) => {
    const recipeExtension = toRecipeExtension(extension);
    if (recipeExtension) {
      extensionMap.set(recipeExtension.name, recipeExtension);
    }
  });

  selectedExtensions.forEach((ext) => {
    const recipeExtension = toRecipeExtension({ ...ext, enabled: true });
    if (recipeExtension) {
      extensionMap.set(recipeExtension.name, recipeExtension);
    }
  });

  const displayExtensions = Array.from(extensionMap.values());

  const handleToggle = (extensionConfig: DisplayRecipeExtension) => {
    const isSelected = selectedExtensionNames.has(extensionConfig.name);

    if (isSelected) {
      onExtensionsChange(selectedExtensions.filter((ext) => ext.name !== extensionConfig.name));
    } else {
      onExtensionsChange([...selectedExtensions, removeDisplayFields(extensionConfig)]);
    }
  };

  const filteredExtensions = displayExtensions.filter((ext) => {
    const query = searchQuery.toLowerCase();
    return (
      ext.name.toLowerCase().includes(query) ||
      (ext.description && ext.description.toLowerCase().includes(query))
    );
  });

  const sortedExtensions = [...filteredExtensions].sort((a, b) => {
    const aSelected = selectedExtensionNames.has(a.name);
    const bSelected = selectedExtensionNames.has(b.name);

    if (aSelected !== bSelected) return aSelected ? -1 : 1;

    return a.name.localeCompare(b.name);
  });

  const activeCount = selectedExtensions.length;

  return (
    <div className="space-y-4">
      <div>
        <label className="block text-md text-textProminent mb-2 font-bold">
          {intl.formatMessage(i18n.label)}
        </label>
        <p className="text-textSubtle text-sm mb-4">{intl.formatMessage(i18n.description)}</p>

        <Input
          type="text"
          placeholder={intl.formatMessage(i18n.searchPlaceholder)}
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          className="mb-3"
        />

        <p className="text-xs text-textSubtle mb-3 text-right">
          {intl.formatMessage(i18n.extensionsSelected, { count: activeCount })}
        </p>
      </div>

      <div className="max-h-[300px] overflow-y-auto border border-borderSubtle rounded-lg">
        {sortedExtensions.length === 0 ? (
          <div className="px-4 py-6 text-center text-sm text-textSubtle">
            {searchQuery
              ? intl.formatMessage(i18n.noExtensionsFound)
              : intl.formatMessage(i18n.noExtensionsAvailable)}
          </div>
        ) : (
          sortedExtensions.map((ext) => {
            const isSelected = selectedExtensionNames.has(ext.name);
            return (
              <div
                key={ext.name}
                className="flex items-center justify-between px-4 py-3 hover:bg-bgSubtle transition-colors cursor-pointer border-b border-borderSubtle last:border-b-0"
                role="button"
                tabIndex={0}
                aria-pressed={isSelected}
                onClick={() => handleToggle(ext)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' || event.key === ' ') {
                    event.preventDefault();
                    handleToggle(ext);
                  }
                }}
                title={ext.description || ext.name}
              >
                <div className="flex-1 min-w-0">
                  <div className="text-sm font-medium text-textStandard">
                    {formatExtensionName(ext.name)}
                  </div>
                  {ext.description && (
                    <div className="text-xs text-textSubtle truncate mt-1">{ext.description}</div>
                  )}
                </div>
                <div onClick={(e) => e.stopPropagation()} className="ml-4">
                  <Switch
                    checked={isSelected}
                    onCheckedChange={() => handleToggle(ext)}
                    variant="mono"
                  />
                </div>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
};
