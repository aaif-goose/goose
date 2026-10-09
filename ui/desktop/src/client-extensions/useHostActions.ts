import { useEffect, useMemo, useRef } from 'react';
import { useNavigate } from 'react-router';
import { useNavigation } from '../hooks/useNavigation';
import { createSession, startNewSession } from '../sessions';
import { getEffectiveWorkingDir } from '../utils/workingDir';
import type { HostActions } from './hostCapabilities';
import { clientExtensionViewPath } from './routes';

export function useHostActions(): HostActions {
  const navigate = useNavigate();
  const setView = useNavigation();
  const latest = useRef({ navigate, setView });

  useEffect(() => {
    latest.current = { navigate, setView };
  });

  return useMemo<HostActions>(
    () => ({
      startChat: async ({ prompt, recipeId, workingDir }) => {
        const session = await startNewSession(
          prompt,
          latest.current.setView,
          workingDir ?? (await getEffectiveWorkingDir()),
          recipeId ? { recipeId } : undefined
        );
        return session.id;
      },
      createSession: async (workingDir) => {
        const session = await createSession(workingDir ?? (await getEffectiveWorkingDir()));
        return session.id;
      },
      openSession: (sessionId) => latest.current.setView('pair', { resumeSessionId: sessionId }),
      openPage: (extensionId, viewId) =>
        latest.current.navigate(clientExtensionViewPath(extensionId, viewId)),
    }),
    []
  );
}
