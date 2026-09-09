/**
 * React hooks for frontend-specific analytics tracking.
 *
 * NOTE: The backend (posthog.rs) already tracks:
 * - session_started (extensions, provider, model, tokens, etc.)
 * - error (provider errors like rate_limit, auth, etc.)
 *
 * These frontend hooks focus on UI-specific events that the backend can't see:
 * - Page views and navigation patterns
 * - Onboarding funnel (where users drop off)
 * - Frontend crashes/errors (React errors, unhandled rejections)
 */

import { useEffect, useRef } from 'react';
import { useLocation } from 'react-router';
import { trackPageView } from '../utils/analytics';

export function usePageViewTracking(): void {
  const location = useLocation();
  const previousPath = useRef<string | null>(null);

  useEffect(() => {
    const currentPath = location.pathname;
    if (currentPath !== previousPath.current) {
      trackPageView(currentPath, previousPath.current || undefined);
      previousPath.current = currentPath;
    }
  }, [location.pathname]);
}
