import type { ExtensionConfig } from '../../../api/types.gen';
import { toastService } from '../../../toasts';
import {
  trackExtensionAdded,
  trackExtensionEnabled,
  trackExtensionDisabled,
  trackExtensionDeleted,
  getErrorType,
} from '../../../utils/analytics';

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

interface ToggleExtensionDefaultProps {
  toggle: 'toggleOn' | 'toggleOff';
  extensionConfig: ExtensionConfig;
  addToConfig: (name: string, extensionConfig: ExtensionConfig, enabled: boolean) => Promise<void>;
}

export async function toggleExtensionDefault({
  toggle,
  extensionConfig,
  addToConfig,
}: ToggleExtensionDefaultProps) {
  const isBuiltin = isBuiltinExtension(extensionConfig);
  const enabled = toggle === 'toggleOn';

  try {
    await addToConfig(extensionConfig.name, extensionConfig, enabled);
    if (enabled) {
      trackExtensionEnabled(extensionConfig.name, true, undefined, isBuiltin);
    } else {
      trackExtensionDisabled(extensionConfig.name, true, undefined, isBuiltin);
    }
    toastService.success({
      title: extensionConfig.name,
      msg: enabled ? 'Extension enabled in defaults' : 'Extension removed from defaults',
    });
  } catch (error) {
    console.error('Failed to update extension default in config:', error);
    if (enabled) {
      trackExtensionEnabled(extensionConfig.name, false, getErrorType(error), isBuiltin);
    } else {
      trackExtensionDisabled(extensionConfig.name, false, getErrorType(error), isBuiltin);
    }
    toastService.error({
      title: extensionConfig.name,
      msg: 'Failed to update extension default',
    });
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
    await addToConfig(extensionConfig.name, extensionConfig, true);
    trackExtensionAdded(extensionConfig.name, true, undefined, isBuiltin);
    toastService.success({
      title: extensionConfig.name,
      msg: 'Extension added as default',
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
