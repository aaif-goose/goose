import { useCallback, useEffect, useState } from 'react';
import { getAnalytics } from '../../api/sdk.gen';
import type { CopilotAnalytics } from '../../api/types.gen';

export type AnalyticsState =
  | { kind: 'idle' }
  | { kind: 'loading' }
  | { kind: 'loaded'; data: CopilotAnalytics }
  | { kind: 'error'; error: string };

export function useCopilotAnalytics(enabled: boolean) {
  const [state, setState] = useState<AnalyticsState>({ kind: 'idle' });

  const refresh = useCallback(async () => {
    setState({ kind: 'loading' });
    try {
      const { data, error, response } = await getAnalytics();
      if (error) {
        const detail =
          typeof error === 'object' && error && 'message' in error
            ? String((error as { message?: unknown }).message ?? '')
            : '';
        throw new Error(`${response?.status ?? '???'}${detail ? `: ${detail}` : ''}`);
      }
      if (!data) throw new Error(`empty response (HTTP ${response?.status ?? '???'})`);
      setState({ kind: 'loaded', data });
    } catch (e) {
      setState({ kind: 'error', error: e instanceof Error ? e.message : String(e) });
    }
  }, []);

  useEffect(() => {
    if (!enabled) return;
    void refresh();
  }, [enabled, refresh]);

  return { state, refresh };
}
