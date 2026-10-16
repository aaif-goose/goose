import type { NavigateFunction } from 'react-router';
import type { FixedExtensionEntry } from '../components/ConfigContext';
import type { Recipe } from '../recipe';
import type { ExtensionConfig } from '../types/extensions';
import { UserInput } from '../types/message';

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
  /** Set when the user picked a directory; otherwise Pair resolves the effective cwd. */
  userSelectedWorkingDir?: boolean;
  extensionConfigs?: ExtensionConfig[];
  /** Set when the user customized next-chat extensions; otherwise Pair uses defaults. */
  userCustomizedExtensions?: boolean;
  allExtensions?: FixedExtensionEntry[];
};

/** Hub input plus the options a failed session/new must restore on remount. */
export interface HubDraft extends UserInput {
  userSelectedWorkingDir?: string;
  extensionConfigs?: ExtensionConfig[];
}

export const emptyHubDraft = (): HubDraft => ({ msg: '', images: [] });

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
