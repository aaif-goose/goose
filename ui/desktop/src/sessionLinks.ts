import { fetchSharedSessionDetails, SharedSessionDetails } from './sharedSessions';
import { View, ViewOptions } from './utils/navigationUtils';
import { errorMessage } from './utils/conversionUtils';
import { importSessionNostr } from './api';

/**
 * Imports a session from an encrypted Nostr deep link.
 * Separated from shared-session handling so callers can route independently.
 */
export async function importNostrSessionFromDeepLink(url: string): Promise<void> {
  await importSessionNostr({
    body: { deeplink: url },
    throwOnError: true,
  });
}

/**
 * Handles opening a shared session from a deep link
 * @param url The deep link URL (goose://sessions/:shareToken)
 * @param setView Function to set the current view
 * @param baseUrl Optional base URL for the session sharing API
 * @returns Promise that resolves when the session is opened
 */
export async function openSharedSessionFromDeepLink(
  url: string,
  setView: (view: View, options?: ViewOptions) => void,
  baseUrl?: string
): Promise<SharedSessionDetails | null> {
  try {
    if (!url.startsWith('goose://sessions/')) {
      throw new Error('Invalid URL: URL must use the goose://sessions/ scheme');
    }

    // Extract the share token from the URL
    const shareToken: string = url.replace('goose://sessions/', '');

    if (!shareToken || shareToken.trim() === '') {
      throw new Error('Invalid URL: Missing share token');
    }

    // If no baseUrl is provided, check if there's one in settings
    if (!baseUrl) {
      const config = await window.electron.getSetting('sessionSharing');
      if (config.enabled && config.baseUrl) {
        baseUrl = config.baseUrl;
      } else {
        throw new Error(
          'Session sharing is not enabled or base URL is not configured. Check the settings page.'
        );
      }
    }

    // Fetch the shared session details
    const sessionDetails = await fetchSharedSessionDetails(baseUrl!, shareToken);

    // Navigate to the shared session view
    setView('sharedSession', {
      sessionDetails,
      shareToken,
      baseUrl,
    });

    return sessionDetails;
  } catch (error) {
    const errMsg = errorMessage(error, 'Unknown error');
    const fullErrorMessage = `Failed to open shared session: ${errMsg}`;
    console.error(fullErrorMessage);

    // Navigate to the shared session view with the error instead of throwing
    setView('sharedSession', {
      sessionDetails: null,
      error: errMsg,
      shareToken: url.replace('goose://sessions/', ''),
      baseUrl,
    });

    return null;
  }
}
