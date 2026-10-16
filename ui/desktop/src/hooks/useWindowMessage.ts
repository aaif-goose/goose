import { useEffect } from 'react';

export function useWindowMessage(handler: (event: MessageEvent) => void) {
  useEffect(() => {
    window.addEventListener('message', handler);
    return () => window.removeEventListener('message', handler);
  }, [handler]);
}
