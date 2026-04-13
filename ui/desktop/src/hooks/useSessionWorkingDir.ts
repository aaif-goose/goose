import { useEffect, useState } from 'react';
import { getSession } from '../api';

/**
 * Session-scoped working directory state with stale async response protection.
 */
export function useSessionWorkingDir(sessionId: string | null) {
  const [sessionWorkingDir, setSessionWorkingDir] = useState<string | null>(null);

  useEffect(() => {
    if (!sessionId) {
      setSessionWorkingDir(null);
      return;
    }

    let cancelled = false;

    const fetchSessionWorkingDir = async () => {
      try {
        const response = await getSession({ path: { session_id: sessionId } });
        if (cancelled) return;

        if (response.data?.working_dir) {
          setSessionWorkingDir(response.data.working_dir);
        } else {
          setSessionWorkingDir(null);
        }
      } catch (error) {
        if (!cancelled) {
          console.error('[useSessionWorkingDir] Failed to fetch session working dir:', error);
        }
      }
    };

    fetchSessionWorkingDir();

    return () => {
      cancelled = true;
    };
  }, [sessionId]);

  return { sessionWorkingDir, setSessionWorkingDir };
}
