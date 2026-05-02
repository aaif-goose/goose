import type { ExtensionConfig } from '../../../api/types.gen';
import { toastService } from '../../../toasts';
import { trackExtensionAdded, trackExtensionDeleted, getErrorType } from '../../../utils/analytics';

function isBuiltinExtension(config: ExtensionConfig): boolean {
  return config.type === 'builtin';
}

interface DeleteExtensionProps {
  name: string;
  removeFromConfig: (name: string) => Promise<void>;
  extensionConfig?: ExtensionConfig;
}

/**
 * Deletes an extension from config (will no longer be loaded in new sessions)
 */
export async function deleteExtension({
  name,
  removeFromConfig,
  extensionConfig,
}: DeleteExtensionProps) {
  const isBuiltin = extensionConfig ? isBuiltinExtension(extensionConfig) : false;

  try {
    await removeFromConfig(name);
    trackExtensionDeleted(name, true, undefined, isBuiltin);
  } catch (error) {
    console.error('Failed to remove extension from config:', error);
    trackExtensionDeleted(name, false, getErrorType(error), isBuiltin);
    throw error;
  }
}

interface ActivateExtensionDefaultProps {
  addToConfig: (name: string, extensionConfig: ExtensionConfig, enabled: boolean) => Promise<void>;
  extensionConfig: ExtensionConfig;
}

export async function activateExtensionDefault({
  addToConfig,
  extensionConfig,
}: ActivateExtensionDefaultProps): Promise<void> {
  const isBuiltin = isBuiltinExtension(extensionConfig);

  try {
    await addToConfig(extensionConfig.name, extensionConfig, false);
    trackExtensionAdded(extensionConfig.name, true, undefined, isBuiltin);
    toastService.success({
      title: extensionConfig.name,
      msg: 'Extension added',
    });
  } catch (error) {
    console.error('Failed to add extension to config:', error);
    trackExtensionAdded(extensionConfig.name, false, getErrorType(error), isBuiltin);
    toastService.error({
      title: extensionConfig.name,
      msg: 'Failed to add extension',
    });
    throw error;
  }
}
