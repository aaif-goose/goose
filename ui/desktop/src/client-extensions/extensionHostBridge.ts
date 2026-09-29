import { toastService } from '../toasts';
import type { HostSession } from './hostCapabilities';
import type { ExtensionToHostMessage } from './messages';
import type { HostToExtensionMessage } from './types';

export function notifyExtensionActivate(
  iframe: HTMLIFrameElement | null,
  message: HostToExtensionMessage,
  hostSession?: HostSession | null
): void {
  if (!iframe?.contentWindow) {
    return;
  }

  iframe.contentWindow.postMessage(message, '*');
  hostSession?.notifyPermissions();
}

export async function routeExtensionToHostMessage(
  hostSession: HostSession,
  message: ExtensionToHostMessage,
  toastTitle: string
): Promise<boolean> {
  if (message.type === 'grc/host/invoke') {
    await hostSession.handleInvoke(message);
    return true;
  }

  if (message.type === 'grc/ui/showMessage') {
    toastService.success({
      title: toastTitle,
      msg: message.text,
    });
    return true;
  }

  return false;
}
