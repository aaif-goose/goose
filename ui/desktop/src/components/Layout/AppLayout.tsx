import React, { useEffect, useState } from 'react';
import { IpcRendererEvent } from 'electron';
import { Outlet, useLocation } from 'react-router-dom';
import { motion } from 'framer-motion';
import { Menu } from 'lucide-react';
import { defineMessages, useIntl } from '../../i18n';
import { Button } from '../ui/button';
import ChatSessionsContainer from '../ChatSessionsContainer';
import { useChatContext } from '../../contexts/ChatContext';
import { NavigationProvider, useNavigationContext } from './NavigationContext';
import { Navigation } from './NavigationPanel';
import { NAV_DIMENSIONS, Z_INDEX } from './constants';
import { cn } from '../../utils';
import { UserInput } from '../../types/message';

const i18n = defineMessages({
  openNavigation: {
    id: 'appLayout.openNavigation',
    defaultMessage: 'Open navigation',
  },
});

interface AppLayoutContentProps {
  activeSessions: Array<{
    sessionId: string;
    initialMessage?: UserInput;
  }>;
}

const AppLayoutContent: React.FC<AppLayoutContentProps> = ({ activeSessions }) => {
  const intl = useIntl();
  const location = useLocation();
  const safeIsMacOS = (window?.electron?.platform || 'darwin') === 'darwin';
  const chatContext = useChatContext();
  const isOnPairRoute = location.pathname === '/pair';

  const [isFullScreen, setIsFullScreen] = useState(false);

  useEffect(() => {
    if (!safeIsMacOS) return;
    window.electron
      .getIsFullScreen()
      .then(setIsFullScreen)
      .catch(() => {});
    const handler = (_event: IpcRendererEvent, ...args: unknown[]) => {
      setIsFullScreen(Boolean(args[0]));
    };
    window.electron.on('fullscreen-change', handler);
    return () => window.electron.off('fullscreen-change', handler);
  }, [safeIsMacOS]);

  const { isNavExpanded, setIsNavExpanded } = useNavigationContext();

  if (!chatContext) {
    throw new Error('AppLayoutContent must be used within ChatProvider');
  }

  const { setChat } = chatContext;

  const needsTrafficLightInset = safeIsMacOS && !isFullScreen;
  const headerPadding = needsTrafficLightInset ? 'pl-[96px]' : 'pl-4';
  const headerTop = needsTrafficLightInset ? 'top-[15px]' : 'top-[11px]';

  return (
    <div className="flex flex-1 w-full h-full relative animate-fade-in bg-background-secondary flex-row">
      {/* Floating menu toggle — only when sidebar is collapsed. When expanded,
          the sidebar's own header has the collapse button. */}
      {!isNavExpanded && (
        <div
          style={{ zIndex: Z_INDEX.HEADER }}
          className={cn('absolute flex items-center gap-1', headerPadding, headerTop, 'ml-1.5')}
        >
          <Button
            onClick={() => setIsNavExpanded(true)}
            className="no-drag hover:!bg-background-tertiary"
            variant="ghost"
            size="xs"
            title={intl.formatMessage(i18n.openNavigation)}
          >
            <Menu className="w-5 h-5" />
          </Button>
        </div>
      )}

      {/* Main content with navigation */}
      <div className="flex flex-1 w-full h-full min-h-0 p-[2px] flex-row">
        <motion.div
          key="nav"
          initial={false}
          animate={{ width: isNavExpanded ? NAV_DIMENSIONS.NAV_WIDTH : 0 }}
          transition={{ type: 'spring', stiffness: 400, damping: 40 }}
          style={{ height: '100%' }}
          className="relative flex-shrink-0 overflow-hidden h-full"
        >
          <div className="w-full h-full overflow-hidden">
            <Navigation />
          </div>
        </motion.div>

        {/* Main content */}
        <div className="flex-1 overflow-hidden min-h-0">
          <div className="h-full w-full bg-background-primary rounded-lg overflow-hidden">
            <Outlet />
            {/* Always render ChatSessionsContainer to keep SSE connections alive.
                When navigating away from /pair, hide it with CSS */}
            <div className={isOnPairRoute ? 'contents' : 'hidden'}>
              <ChatSessionsContainer setChat={setChat} activeSessions={activeSessions} />
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

interface AppLayoutProps {
  activeSessions: Array<{
    sessionId: string;
    initialMessage?: UserInput;
  }>;
}

export const AppLayout: React.FC<AppLayoutProps> = ({ activeSessions }) => {
  return (
    <NavigationProvider>
      <AppLayoutContent activeSessions={activeSessions} />
    </NavigationProvider>
  );
};
