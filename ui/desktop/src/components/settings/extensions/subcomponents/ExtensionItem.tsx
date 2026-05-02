import kebabCase from 'lodash/kebabCase';
import { Gear } from '../../../icons';
import { FixedExtensionEntry } from '../../../ConfigContext';
import { getSubtitle, getFriendlyTitle } from './ExtensionList';
import { Card, CardHeader, CardTitle, CardContent, CardAction } from '../../../ui/card';
import { defineMessages, useIntl } from '../../../../i18n';

const i18n = defineMessages({
  configureExtension: {
    id: 'extensionItem.configureExtension',
    defaultMessage: 'Configure {name} Extension',
  },
});

interface ExtensionItemProps {
  extension: FixedExtensionEntry;
  onConfigure?: (extension: FixedExtensionEntry) => void;
  isStatic?: boolean; // to not allow users to edit configuration
}

export default function ExtensionItem({ extension, onConfigure, isStatic }: ExtensionItemProps) {
  const intl = useIntl();

  const renderSubtitle = () => {
    const { description, command } = getSubtitle(extension);
    return (
      <>
        {description && <span>{description}</span>}
        {description && command && <br />}
        {command && <span className="font-mono text-xs">{command}</span>}
      </>
    );
  };

  // Bundled extensions and builtins are not editable
  // Over time we can take the first part of the conditional away as people have bundled: true in their config.yaml entries

  // allow configuration editing if extension is not a builtin/bundled extension AND isStatic = false
  const editable =
    !(extension.type === 'builtin' || ('bundled' in extension && extension.bundled)) && !isStatic;

  return (
    <Card
      id={`extension-${kebabCase(extension.name)}`}
      className="transition-all duration-200 min-h-[120px] overflow-hidden"
    >
      <CardHeader>
        <CardTitle>{getFriendlyTitle(extension)}</CardTitle>

        <CardAction>
          <div className="flex items-center justify-end gap-2">
            {editable && (
              <button
                type="button"
                className="text-text-secondary hover:text-text-primary"
                aria-label={intl.formatMessage(i18n.configureExtension, {
                  name: getFriendlyTitle(extension),
                })}
                onClick={() => onConfigure?.(extension)}
              >
                <Gear className="w-4 h-4" />
              </button>
            )}
          </div>
        </CardAction>
      </CardHeader>
      <CardContent className="px-4 overflow-hidden text-sm break-words text-text-secondary">
        {renderSubtitle()}
      </CardContent>
    </Card>
  );
}
