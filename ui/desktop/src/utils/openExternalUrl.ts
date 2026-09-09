import { dialog, shell, type BrowserWindow, type MessageBoxOptions } from 'electron';
import { getExternalLinkLabels } from './externalLinkTranslations';
import { BLOCKED_PROTOCOLS, SAFE_PROTOCOLS, type OpenExternalUrlResult } from './urlSecurity';

export const openExternalUrl = async (
  url: string,
  parentWindow?: BrowserWindow,
  locale?: string
): Promise<OpenExternalUrlResult> => {
  let protocol: string;
  try {
    protocol = new URL(url).protocol;
  } catch {
    return 'blocked';
  }

  if (BLOCKED_PROTOCOLS.includes(protocol)) return 'blocked';

  if (!SAFE_PROTOCOLS.includes(protocol)) {
    const labels = getExternalLinkLabels(locale);
    const options: MessageBoxOptions = {
      type: 'warning',
      buttons: [labels.cancel, labels.open],
      defaultId: 0,
      cancelId: 0,
      title: labels.title,
      message: labels.message.replace('{protocol}', protocol),
      detail: labels.detail.replace('{href}', url),
    };
    const result = parentWindow
      ? await dialog.showMessageBox(parentWindow, options)
      : await dialog.showMessageBox(options);
    if (result.response !== 1) return 'cancelled';
  }

  await shell.openExternal(url);
  return 'opened';
};
