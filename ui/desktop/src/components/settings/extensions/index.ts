export { DEFAULT_EXTENSION_TIMEOUT, nameToKey } from './utils';

export {
  activateExtensionDefault,
  toggleExtensionDefault,
  deleteExtension,
} from './extension-manager';

export { pruneDeprecatedBundledExtensions, syncBundledExtensions } from './bundled-extensions';

export { addExtensionFromDeepLink } from './deeplink';

export { addToAgent, removeFromAgent } from './agent-api';
