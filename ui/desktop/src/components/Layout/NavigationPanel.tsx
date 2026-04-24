import React, { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import { useLocation } from 'react-router-dom';
import { useNavigationContext } from './NavigationContext';
import { useConfig } from '../ConfigContext';
import { useNavigationSessions } from '../../hooks/useNavigationSessions';
import { getNavItemById, type NavItem } from '../../hooks/useNavigationItems';
import { AppEvents } from '../../constants/events';
import { CondensedRenderer } from './CondensedRenderer';
import { ExpandedRenderer } from './ExpandedRenderer';
import { NavigationOverlay } from './navigation';
import type { SessionStatus, DragHandlers } from './navigation/types';

export const Navigation: React.FC<{ className?: string }> = ({ className }) => {
  const {
    isNavExpanded,
    setIsNavExpanded,
    navigationPosition,
    preferences,
    updatePreferences,
    isCondensedIconOnly,
    isOverlayMode,
    effectiveNavigationStyle,
    isChatExpanded,
    setIsChatExpanded,
  } = useNavigationContext();

  const location = useLocation();
  const { extensionsList } = useConfig();

  const appsExtensionEnabled = !!extensionsList?.find((ext) => ext.name === 'apps')?.enabled;

  const visibleItems = useMemo(() => {
    return preferences.itemOrder
      .filter((id) => preferences.enabledItems.includes(id))
      .map((id) => getNavItemById(id))
      .filter((item): item is NavItem => item !== undefined)
      .filter((item) => {
        if (item.path === '/apps') return appsExtensionEnabled;
        return true;
      });
  }, [preferences.itemOrder, preferences.enabledItems, appsExtensionEnabled]);

  const isActive = useCallback((path: string) => location.pathname === path, [location.pathname]);

  const {
    recentSessions,
    recentSessionsByProject,
    activeSessionId,
    fetchSessions,
    handleNavClick,
    handleNewChat,
    handleSessionClick,
  } = useNavigationSessions({
    onNavigate: isOverlayMode ? () => setIsNavExpanded(false) : undefined,
  });

  const [draggedItem, setDraggedItem] = useState<string | null>(null);
  const [dragOverItem, setDragOverItem] = useState<string | null>(null);

  const onDragStart = useCallback((e: React.DragEvent, itemId: string) => {
    setDraggedItem(itemId);
    e.dataTransfer.effectAllowed = 'move';
  }, []);

  const onDragOver = useCallback(
    (e: React.DragEvent, itemId: string) => {
      e.preventDefault();
      if (draggedItem && draggedItem !== itemId) setDragOverItem(itemId);
    },
    [draggedItem]
  );

  const onDrop = useCallback(
    (e: React.DragEvent, dropItemId: string) => {
      e.preventDefault();
      if (!draggedItem || draggedItem === dropItemId) return;

      const newOrder = [...preferences.itemOrder];
      const draggedIndex = newOrder.indexOf(draggedItem);
      const dropIndex = newOrder.indexOf(dropItemId);
      if (draggedIndex === -1 || dropIndex === -1) return;

      newOrder.splice(draggedIndex, 1);
      newOrder.splice(dropIndex, 0, draggedItem);
      updatePreferences({ ...preferences, itemOrder: newOrder });

      setDraggedItem(null);
      setDragOverItem(null);
    },
    [draggedItem, preferences, updatePreferences]
  );

  const onDragEnd = useCallback(() => {
    setDraggedItem(null);
    setDragOverItem(null);
  }, []);

  const drag: DragHandlers = {
    draggedItem,
    dragOverItem,
    onDragStart,
    onDragOver,
    onDrop,
    onDragEnd,
  };

  const [sessionStatuses, setSessionStatuses] = useState<Map<string, SessionStatus>>(new Map());

  useEffect(() => {
    const handleStatusUpdate = (event: Event) => {
      const { sessionId, streamState } = (event as CustomEvent).detail;
      setSessionStatuses((prev) => {
        const existing = prev.get(sessionId);
        const shouldMarkUnread = existing?.streamState === 'streaming' && streamState === 'idle';
        const next = new Map(prev);
        next.set(sessionId, {
          streamState,
          hasUnreadActivity: existing?.hasUnreadActivity || shouldMarkUnread,
        });
        return next;
      });
    };

    window.addEventListener(AppEvents.SESSION_STATUS_UPDATE, handleStatusUpdate);
    return () => window.removeEventListener(AppEvents.SESSION_STATUS_UPDATE, handleStatusUpdate);
  }, []);

  const getSessionStatus = useCallback(
    (sessionId: string) => sessionStatuses.get(sessionId),
    [sessionStatuses]
  );

  const clearUnread = useCallback((sessionId: string) => {
    setSessionStatuses((prev) => {
      const status = prev.get(sessionId);
      if (status?.hasUnreadActivity) {
        const next = new Map(prev);
        next.set(sessionId, { ...status, hasUnreadActivity: false });
        return next;
      }
      return prev;
    });
  }, []);

  useEffect(() => {
    if (!(isOverlayMode && isNavExpanded)) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        setIsNavExpanded(false);
      }
    };

    document.addEventListener('keydown', handleKeyDown, { capture: true });
    return () => document.removeEventListener('keydown', handleKeyDown, { capture: true });
  }, [isNavExpanded, isOverlayMode, setIsNavExpanded]);

  const navFocusRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (isNavExpanded) {
      fetchSessions();
      requestAnimationFrame(() => navFocusRef.current?.focus());
    }
  }, [isNavExpanded, fetchSessions]);

  const onToggleChatExpanded = useCallback(() => {
    setIsChatExpanded(!isChatExpanded);
  }, [isChatExpanded, setIsChatExpanded]);

  const onClose = useCallback(() => setIsNavExpanded(false), [setIsNavExpanded]);

  const rendererProps = {
    isNavExpanded,
    isOverlayMode,
    navigationPosition,
    isCondensedIconOnly,
    onClose,
    className,
    visibleItems,
    isActive,
    recentSessions,
    recentSessionsByProject,
    activeSessionId,
    onNavClick: handleNavClick,
    onNewChat: handleNewChat,
    onSessionClick: handleSessionClick,
    onFetchSessions: fetchSessions,
    getSessionStatus,
    clearUnread,
    isChatExpanded,
    onToggleChatExpanded,
    drag,
    navFocusRef,
  };

  const content =
    effectiveNavigationStyle === 'expanded' ? (
      <ExpandedRenderer {...rendererProps} />
    ) : (
      <CondensedRenderer {...rendererProps} />
    );

  if (isOverlayMode) {
    if (effectiveNavigationStyle === 'expanded') {
      // Expanded overlay uses its own AnimatePresence layout
      return content;
    }
    return (
      <NavigationOverlay
        isOpen={isNavExpanded}
        position={navigationPosition}
        onClose={() => setIsNavExpanded(false)}
      >
        {content}
      </NavigationOverlay>
    );
  }

  if (!isNavExpanded) return null;
  return content;
};
