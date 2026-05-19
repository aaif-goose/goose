import type { NavItem } from '../../../hooks/useNavigationItems';
import type { SessionListItem } from '../../../acp/sessions';
import type { NavigationPosition } from '../NavigationContext';

export type StreamState = 'idle' | 'loading' | 'streaming' | 'error';

export interface SessionStatus {
  streamState: StreamState;
  hasUnreadActivity: boolean;
}

export interface DragHandlers {
  draggedItem: string | null;
  dragOverItem: string | null;
  onDragStart: (e: React.DragEvent, itemId: string) => void;
  onDragOver: (e: React.DragEvent, itemId: string) => void;
  onDrop: (e: React.DragEvent, dropItemId: string) => void;
  onDragEnd: () => void;
}

export interface NavigationRendererProps {
  isNavExpanded: boolean;
  isOverlayMode: boolean;
  navigationPosition: NavigationPosition;
  isCondensedIconOnly: boolean;
  onClose: () => void;
  className?: string;

  // Items
  visibleItems: NavItem[];
  isActive: (path: string) => boolean;

  // Sessions
  recentSessions: SessionListItem[];
  activeSessionId?: string;
  onNavClick: (path: string) => void;
  onNewChat: () => void;
  onSessionClick: (sessionId: string) => void;
  onFetchSessions: () => void;

  // Session status
  getSessionStatus: (sessionId: string) => SessionStatus | undefined;
  clearUnread: (sessionId: string) => void;

  // Chat expand (condensed only, but simpler to keep uniform)
  isChatExpanded: boolean;
  onToggleChatExpanded: () => void;

  // Drag and drop
  drag: DragHandlers;

  // Ref for focus management
  navFocusRef: React.RefObject<HTMLDivElement | null>;
}
