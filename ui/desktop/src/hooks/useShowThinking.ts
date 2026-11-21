import { useEffect, useState } from 'react';
import { AppEvents } from '../constants/events';
import type { ShowThinking } from '../utils/settings';

export function useShowThinking() {
  const [showThinking, setShowThinking] = useState<ShowThinking | null>(null);

  useEffect(() => {
    const loadShowThinking = () => {
      window.electron.getSetting('showThinking').then(setShowThinking);
    };

    loadShowThinking();
    window.addEventListener(AppEvents.SHOW_THINKING_CHANGED, loadShowThinking);

    return () => {
      window.removeEventListener(AppEvents.SHOW_THINKING_CHANGED, loadShowThinking);
    };
  }, []);

  return showThinking;
}
