import { useEffect, useState } from 'react';
import { defineMessages, useIntl } from '../../i18n';
import { errorMessage } from '../../utils/conversionUtils';
import { Card, CardContent, CardDescription } from '../ui/card';
import { Greeting } from '../common/Greeting';
import { useNavigate } from 'react-router-dom';
import { Button } from '../ui/button';
import { ChatSmart } from '../icons/';
import { Goose } from '../icons/Goose';
import { Skeleton } from '../ui/skeleton';
import {
  getSessionInsights,
  listSessions,
  Session,
  SessionInsights as ApiSessionInsights,
} from '../../api';
import { resumeSession } from '../../sessions';
import { useNavigation } from '../../hooks/useNavigation';

const i18n = defineMessages({
  totalSessions: {
    id: 'sessionsInsights.totalSessions',
    defaultMessage: 'Total sessions',
  },
  totalTokens: {
    id: 'sessionsInsights.totalTokens',
    defaultMessage: 'Total tokens',
  },
  recentChats: {
    id: 'sessionsInsights.recentChats',
    defaultMessage: 'Recent chats',
  },
  seeAll: {
    id: 'sessionsInsights.seeAll',
    defaultMessage: 'See all',
  },
  noRecentChats: {
    id: 'sessionsInsights.noRecentChats',
    defaultMessage: 'No recent chat sessions found.',
  },
  failedToLoadInsights: {
    id: 'sessionsInsights.failedToLoad',
    defaultMessage: 'Failed to load insights',
  },
});

export function SessionInsights() {
  const intl = useIntl();
  const [insights, setInsights] = useState<ApiSessionInsights | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [recentSessions, setRecentSessions] = useState<Session[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [isLoadingSessions, setIsLoadingSessions] = useState(true);
  const navigate = useNavigate();
  const setView = useNavigation();

  useEffect(() => {
    let loadingTimeout: ReturnType<typeof setTimeout>;

    const loadInsights = async () => {
      try {
        const response = await getSessionInsights({ throwOnError: true });
        setInsights(response.data);
        setError(null);
      } catch (error) {
        console.error('Failed to load insights:', error);
        setError(errorMessage(error, 'Failed to load insights'));
        setInsights({
          totalSessions: 0,
          totalTokens: 0,
        });
      } finally {
        setIsLoading(false);
      }
    };

    const loadRecentSessions = async () => {
      try {
        const response = await listSessions<true>({ throwOnError: true });
        setRecentSessions(response.data.sessions.slice(0, 3));
      } finally {
        setIsLoadingSessions(false);
      }
    };

    // Set a maximum loading time to prevent infinite skeleton
    loadingTimeout = setTimeout(() => {
      // Only apply fallback if we still don't have insights data
      setInsights((currentInsights) => {
        if (!currentInsights) {
          console.warn('Loading timeout reached, showing fallback content');
          setError('Failed to load insights');
          setIsLoading(false);
          return {
            totalSessions: 0,
            mostActiveDirs: [],
            avgSessionDuration: 0,
            totalTokens: 0,
            recentActivity: [],
          };
        }
        // If we already have insights, just make sure loading is false
        setIsLoading(false);
        return currentInsights;
      });
    }, 10000); // 10 second timeout

    loadInsights();
    loadRecentSessions();

    return () => {
      if (loadingTimeout) {
        window.clearTimeout(loadingTimeout);
      }
    };
  }, []);

  const handleSessionClick = async (session: Session) => {
    try {
      resumeSession(session, setView);
    } catch (error) {
      console.error('Failed to start session:', error);
      navigate('/sessions', {
        state: { selectedSessionId: session.id },
        replace: true,
      });
    }
  };

  const navigateToSessionHistory = () => {
    navigate('/sessions');
  };

  // Format date to show only the date part (without time)
  const formatDateOnly = (dateStr: string) => {
    const date = new Date(dateStr);
    return date
      .toLocaleDateString('en-US', { month: '2-digit', day: '2-digit', year: 'numeric' })
      .replace(/\//g, '/');
  };

  const formatTokens = (tokens: number | undefined): string => {
    return new Intl.NumberFormat('en', { notation: 'compact', maximumFractionDigits: 2 }).format(
      tokens || 0
    );
  };

  // Render skeleton loader while data is loading
  const renderSkeleton = () => (
    <div className="bg-background-secondary flex flex-col h-full">
      {/* Header container with rounded bottom */}
      <div className="bg-background-primary rounded-b-2xl mb-0.5">
        <div className="px-8 pb-12 pt-19 space-y-4">
          <div className="origin-bottom-left goose-icon-animation">
            <Goose className="size-8" />
          </div>
          <Greeting />
        </div>
      </div>

      {/* Stats containers - full bleed with 2px gaps */}
      <div className="flex flex-col flex-1 space-y-0.5">
        {/* Top row with three equal columns */}
        <div className="grid grid-cols-2 gap-0.5">
          {/* Total Sessions Card Skeleton */}
          <Card className="w-full py-6 px-6 border-none rounded-2xl bg-background-primary">
            <CardContent className="flex flex-col justify-end h-full p-0">
              <div className="flex flex-col justify-end">
                <Skeleton className="h-10 w-16 mb-1" />
                <span className="text-xs text-text-secondary">{intl.formatMessage(i18n.totalSessions)}</span>
              </div>
            </CardContent>
          </Card>

          {/* Total Tokens Card Skeleton */}
          <Card className="w-full py-6 px-6 border-none rounded-2xl bg-background-primary">
            <CardContent className="flex flex-col justify-end h-full p-0">
              <div className="flex flex-col justify-end">
                <Skeleton className="h-10 w-24 mb-1" />
                <span className="text-xs text-text-secondary">{intl.formatMessage(i18n.totalTokens)}</span>
              </div>
            </CardContent>
          </Card>
        </div>

        {/* Recent Chats Card Skeleton */}
        <div className="grid grid-cols-1 gap-0.5">
          <Card className="w-full py-6 px-6 border-none rounded-2xl bg-background-primary">
            <CardContent className="p-0">
              <div className="flex justify-between items-center mb-4">
                <CardDescription className="mb-0">
                  <span className="text-lg text-text-primary">{intl.formatMessage(i18n.recentChats)}</span>
                </CardDescription>
                <Button
                  variant="ghost"
                  size="sm"
                  className="text-xs text-text-secondary flex items-center gap-1 !px-0 hover:bg-transparent hover:underline hover:text-text-primary"
                  onClick={navigateToSessionHistory}
                >
                  {intl.formatMessage(i18n.seeAll)}
                </Button>
              </div>
              <div className="space-y-3 min-h-[96px] max-h-[140px] overflow-hidden">
                {/* Skeleton chat items */}
                <div className="flex items-center justify-between py-1 px-2">
                  <div className="flex items-center space-x-2">
                    <Skeleton className="h-4 w-4 rounded-sm" />
                    <Skeleton className="h-4 w-48" />
                  </div>
                  <Skeleton className="h-4 w-16" />
                </div>
                <div className="flex items-center justify-between py-1 px-2">
                  <div className="flex items-center space-x-2">
                    <Skeleton className="h-4 w-4 rounded-sm" />
                    <Skeleton className="h-4 w-40" />
                  </div>
                  <Skeleton className="h-4 w-16" />
                </div>
                <div className="flex items-center justify-between py-1 px-2">
                  <div className="flex items-center space-x-2">
                    <Skeleton className="h-4 w-4 rounded-sm" />
                    <Skeleton className="h-4 w-52" />
                  </div>
                  <Skeleton className="h-4 w-16" />
                </div>
              </div>
            </CardContent>
          </Card>
        </div>

        {/* Filler container - extends to fill remaining space */}
        <div className="bg-background-primary rounded-2xl flex-1"></div>
      </div>
    </div>
  );

  // Show skeleton while loading, then show actual content
  if (isLoading) {
    return renderSkeleton();
  }

  return (
    <div className="bg-background-secondary flex flex-col h-full">
      {/* Header container with rounded bottom */}
      <div className="bg-background-primary rounded-b-2xl mb-0.5">
        <div className="px-8 pb-12 pt-19 space-y-4">
          <div className="origin-bottom-left goose-icon-animation">
            <Goose className="size-8" />
          </div>
          <Greeting />
        </div>
      </div>

      {/* Stats containers - full bleed with 2px gaps */}
      <div className="flex flex-col flex-1 space-y-0.5">
        {/* Error notice if insights failed to load */}
        {error && (
          <div className="mx-0.5 px-4 py-2 bg-orange-50 dark:bg-orange-950/20 border border-orange-200 dark:border-orange-800/30 rounded-xl">
            <div className="flex items-center space-x-2">
              <div className="w-2 h-2 bg-orange-400 rounded-full flex-shrink-0"></div>
              <span className="text-xs text-orange-700 dark:text-orange-300">
                {intl.formatMessage(i18n.failedToLoadInsights)}
              </span>
            </div>
          </div>
        )}

        {/* Top row with three equal columns */}
        <div className="grid grid-cols-2 gap-0.5">
          {/* Total Sessions Card */}
          <Card className="w-full py-6 px-6 border-none rounded-2xl bg-background-primary">
            <CardContent className="page-transition flex flex-col justify-end h-full p-0">
              <div className="flex flex-col justify-end">
                <p className="text-4xl font-mono font-light flex items-end">
                  {Math.max(insights?.totalSessions ?? 0, 0)}
                </p>
                <span className="text-xs text-text-secondary">{intl.formatMessage(i18n.totalSessions)}</span>
              </div>
            </CardContent>
          </Card>

          {/* Average Duration Card */}
          {/*<Card className="w-full py-6 px-6 border-none rounded-2xl bg-background-primary">*/}
          {/*  <CardContent className="page-transition flex flex-col justify-end h-full p-0">*/}
          {/*    <div className="flex flex-col justify-end">*/}
          {/*      <p className="text-4xl font-mono font-light flex items-end">*/}
          {/*        {insights?.avgSessionDuration*/}
          {/*          ? `${insights.avgSessionDuration.toFixed(1)}m`*/}
          {/*          : '0.0m'}*/}
          {/*      </p>*/}
          {/*      <span className="text-xs text-text-secondary">Avg. chat length</span>*/}
          {/*    </div>*/}
          {/*  </CardContent>*/}
          {/*</Card>*/}

          {/* Total Tokens Card */}
          <Card className="w-full py-6 px-6 border-none rounded-2xl bg-background-primary">
            <CardContent className="page-transition flex flex-col justify-end h-full p-0">
              <div className="flex flex-col justify-end">
                <p className="text-4xl font-mono font-light flex items-end">
                  {formatTokens(insights?.totalTokens)}
                </p>
                <span className="text-xs text-text-secondary">{intl.formatMessage(i18n.totalTokens)}</span>
              </div>
            </CardContent>
          </Card>
        </div>

        {/* Recent Chats Card */}
        <div className="grid grid-cols-1 gap-0.5">
          {/* Recent Chats Card */}
          <Card className="w-full py-6 px-6 border-none rounded-2xl bg-background-primary">
            <CardContent className="page-transition p-0">
              <div className="flex justify-between items-center mb-4">
                <CardDescription className="mb-0">
                  <span className="text-lg text-text-primary">{intl.formatMessage(i18n.recentChats)}</span>
                </CardDescription>
                <Button
                  variant="ghost"
                  size="sm"
                  className="text-xs text-text-secondary flex items-center gap-1 !px-0 hover:bg-transparent hover:underline hover:text-text-primary"
                  onClick={navigateToSessionHistory}
                >
                  {intl.formatMessage(i18n.seeAll)}
                </Button>
              </div>
              <div className="space-y-1 min-h-[96px] max-h-[140px] overflow-hidden transition-all duration-300 ease-in-out">
                {isLoadingSessions ? (
                  <>
                    <div className="flex items-center justify-between py-1 px-2">
                      <div className="flex items-center space-x-2">
                        <Skeleton className="h-4 w-4 rounded-sm" />
                        <Skeleton className="h-4 w-48" />
                      </div>
                      <Skeleton className="h-4 w-16" />
                    </div>
                    <div className="flex items-center justify-between py-1 px-2">
                      <div className="flex items-center space-x-2">
                        <Skeleton className="h-4 w-4 rounded-sm" />
                        <Skeleton className="h-4 w-40" />
                      </div>
                      <Skeleton className="h-4 w-16" />
                    </div>
                    <div className="flex items-center justify-between py-1 px-2">
                      <div className="flex items-center space-x-2">
                        <Skeleton className="h-4 w-4 rounded-sm" />
                        <Skeleton className="h-4 w-52" />
                      </div>
                      <Skeleton className="h-4 w-16" />
                    </div>
                  </>
                ) : recentSessions.length > 0 ? (
                  recentSessions.map((session, index) => (
                    <div
                      key={session.id}
                      className="flex items-center justify-between text-sm py-1 px-2 rounded-md hover:bg-background-secondary cursor-pointer transition-colors"
                      onClick={() => handleSessionClick(session)}
                      role="button"
                      tabIndex={0}
                      style={{ animationDelay: `${index * 0.1}s` }}
                      onKeyDown={async (e) => {
                        if (e.key === 'Enter' || e.key === ' ') {
                          await handleSessionClick(session);
                        }
                      }}
                    >
                      <div className="flex items-center space-x-2">
                        <ChatSmart className="h-4 w-4 text-text-secondary" />
                        <span className="truncate max-w-[300px]">{session.name}</span>
                      </div>
                      <span className="text-text-secondary font-mono font-light">
                        {formatDateOnly(session.updated_at)}
                      </span>
                    </div>
                  ))
                ) : (
                  <div className="text-text-secondary text-sm py-2">
                    {intl.formatMessage(i18n.noRecentChats)}
                  </div>
                )}
              </div>
            </CardContent>
          </Card>
        </div>

        {/* Filler container - extends to fill remaining space */}
        <div className="bg-background-primary rounded-2xl flex-1"></div>
      </div>
    </div>
  );
}
