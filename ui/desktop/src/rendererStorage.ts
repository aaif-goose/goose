import { WebContentsView, type Session } from 'electron';
import type { Settings } from './utils/settings';
import { isAppRendererUrl } from './desktopFileAccess';
import { CHAT_HISTORY_KEY } from './utils/localMessageStorage';

export async function readLegacyRendererStorage(
  session: Session,
  rendererUrl: URL
): Promise<Map<string, string>> {
  const { webContents } = new WebContentsView({ webPreferences: { session, javascript: false } });
  try {
    await webContents.loadURL(rendererUrl.href);
    if (!isAppRendererUrl(webContents.getURL(), rendererUrl)) {
      throw new Error('Legacy renderer storage loaded an unexpected URL');
    }
    webContents.debugger.attach('1.3');
    const { entries } = await webContents.debugger.sendCommand('DOMStorage.getDOMStorageItems', {
      storageId: {
        securityOrigin: rendererUrl.protocol === 'file:' ? 'file://' : rendererUrl.origin,
        isLocalStorage: true,
      },
    });
    return new Map(entries);
  } finally {
    webContents.close();
  }
}

export function migrateRendererStorage(settings: Settings, values: Map<string, string>): void {
  const theme = values.get('theme');
  if (theme === 'dark' || theme === 'light' || theme === 'aura') {
    settings.theme = theme;
  }
  const useSystemTheme = values.get('use_system_theme');
  if (useSystemTheme !== undefined) {
    settings.useSystemTheme = useSystemTheme === 'true';
  }
  const responseStyle = values.get('response_style');
  if (responseStyle !== undefined) {
    settings.responseStyle = responseStyle;
  }
  const showPricing = values.get('show_pricing');
  if (showPricing !== undefined) {
    settings.showPricing = showPricing === 'true';
  }
  try {
    const announcementIds: unknown = JSON.parse(values.get('seenAnnouncementIds') ?? 'null');
    if (Array.isArray(announcementIds) && announcementIds.every((id) => typeof id === 'string')) {
      settings.seenAnnouncementIds = announcementIds;
    }
  } catch {
    // The legacy renderer ignored malformed announcement data.
  }

  const storage = (settings.rendererStorage ??= {});
  for (const key of RENDERER_STORAGE_KEYS) {
    const value = values.get(key);
    if (storage[key] === undefined && value !== undefined) {
      storage[key] = value;
    }
  }
  settings.rendererStorageMigrated = true;
}

export const RENDERER_STORAGE_KEYS = [
  'navigation_expanded',
  'navigation_width',
  'sessions_include_acp',
] as const;

export type RendererStorageKey = (typeof RENDERER_STORAGE_KEYS)[number] | typeof CHAT_HISTORY_KEY;
export type RendererStorage = Partial<Record<RendererStorageKey, string | null>>;
