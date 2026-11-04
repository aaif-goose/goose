import type { NavigateFunction } from 'react-router';
import type { Recipe } from '../recipe';
import type { ExtensionConfig } from '../types/extensions';
import { UserInput } from '../types/message';
import type { NextChatExtensionDraft } from './nextChatExtensions';

export type View =
  | 'chat'
  | 'pair'
  | 'settings'
  | 'extensions'
  | 'moreModels'
  | 'configureProviders'
  | 'configPage'
  | 'ConfigureProviders'
  | 'settingsV2'
  | 'sessions'
  | 'schedules'
  | 'loading'
  | 'recipes'
  | 'skills'
  | 'permission';

export type ViewOptions = {
  showEnvVars?: boolean;
  deepLinkConfig?: unknown;
  error?: string;
  recipe?: Recipe;
  parentView?: View;
  parentViewOptions?: ViewOptions;
  disableAnimation?: boolean;
  initialMessage?: UserInput;
  resumeSessionId?: string;
  startLiveVoice?: boolean;
  pendingScheduleDeepLink?: string;
  workingDir?: string;
  extensionConfigs?: ExtensionConfig[];
  /** Set when this /pair navigation came from a Hub submit, so Pair may clear that draft. */
  fromHub?: boolean;
};

/** Options a failed Hub submit must put back, kept beside the string draft. */
export type HubSubmitSnapshot = {
  text: string;
  images: UserInput['images'];
  workingDir?: string;
  extensionDraft?: NextChatExtensionDraft | null;
};

const hubSubmitSnapshots = new WeakMap<object, HubSubmitSnapshot>();

export function rememberHubSubmit(draftRef: object, snapshot: HubSubmitSnapshot) {
  hubSubmitSnapshots.set(draftRef, snapshot);
}

export function takeHubSubmit(draftRef: object): HubSubmitSnapshot | undefined {
  const snapshot = hubSubmitSnapshots.get(draftRef);
  if (snapshot) hubSubmitSnapshots.delete(draftRef);
  return snapshot;
}

export function peekHubSubmit(draftRef: object): HubSubmitSnapshot | undefined {
  return hubSubmitSnapshots.get(draftRef);
}

export const createNavigationHandler = (navigate: NavigateFunction) => {
  return (view: View, options?: ViewOptions) => {
    switch (view) {
      case 'chat':
        navigate('/', { state: options });
        break;
      case 'pair': {
        // Put resumeSessionId in URL search params (not just state) so that:
        // 1. The sidebar can read it to highlight the active session
        // 2. Page refresh preserves which session is active
        // 3. Browser back/forward navigation works correctly
        const searchParams = new URLSearchParams();
        if (options?.resumeSessionId) {
          searchParams.set('resumeSessionId', options.resumeSessionId);
        }
        const url = searchParams.toString() ? `/pair?${searchParams.toString()}` : '/pair';
        navigate(url, { state: options });
        break;
      }
      case 'settings':
        navigate('/settings', { state: options });
        break;
      case 'sessions':
        navigate('/sessions', { state: options });
        break;
      case 'schedules':
        navigate('/schedules', { state: options });
        break;
      case 'recipes':
        navigate('/recipes', { state: options });
        break;
      case 'skills':
        navigate('/skills', { state: options });
        break;
      case 'permission':
        navigate('/permission', { state: options });
        break;
      case 'ConfigureProviders':
        navigate('/configure-providers', { state: options });
        break;
      case 'extensions':
        navigate('/extensions', { state: options });
        break;
      default:
        navigate('/', { state: options });
    }
  };
};
