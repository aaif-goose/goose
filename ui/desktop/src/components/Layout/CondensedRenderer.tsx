import React, { useState } from 'react';
import { GripVertical, ChevronDown, ChevronRight, Plus } from 'lucide-react';
import { motion } from 'framer-motion';
import { defineMessages, useIntl } from '../../i18n';
import { cn } from '../../utils';
import { DropdownMenu, DropdownMenuTrigger } from '../ui/dropdown-menu';
import { ChatSessionsDropdown, SessionsList } from './navigation';
import type { NavigationRendererProps } from './navigation/types';

const i18n = defineMessages({
  newChat: {
    id: 'condensedRenderer.newChat',
    defaultMessage: 'New Chat',
  },
});

export const CondensedRenderer: React.FC<NavigationRendererProps> = ({
  isOverlayMode,
  navigationPosition,
  isCondensedIconOnly,
  className,
  visibleItems,
  isActive,
  recentSessions,
  activeSessionId,
  onNavClick,
  onNewChat,
  onSessionClick,
  onFetchSessions,
  getSessionStatus,
  clearUnread,
  isChatExpanded,
  onToggleChatExpanded,
  drag,
  navFocusRef,
}) => {
  const intl = useIntl();
  const [chatPopoverOpen, setChatPopoverOpen] = useState(false);

  const isVertical = navigationPosition === 'left' || navigationPosition === 'right';
  const isTopPosition = navigationPosition === 'top';
  const isBottomPosition = navigationPosition === 'bottom';

  return (
    <motion.div
      ref={navFocusRef}
      tabIndex={-1}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.15 }}
      className={cn(
        'bg-app outline-none',
        isOverlayMode && 'rounded-xl backdrop-blur-md shadow-lg p-2',
        isVertical ? 'flex flex-col gap-[2px] h-full' : 'flex flex-row items-stretch gap-[2px]',
        !isOverlayMode && navigationPosition === 'left' && !isCondensedIconOnly && 'pr-[2px]',
        !isOverlayMode && navigationPosition === 'right' && !isCondensedIconOnly && 'pl-[2px]',
        !isOverlayMode && isTopPosition && 'pb-[2px] pt-0',
        !isOverlayMode && isBottomPosition && 'pt-[2px] pb-0',
        !isCondensedIconOnly && 'overflow-visible',
        className
      )}
    >
      {/* Top spacer (vertical only) */}
      {isVertical && (
        <div
          className={cn(
            'bg-background-primary rounded-lg flex-shrink-0',
            isCondensedIconOnly ? 'h-[80px] w-[40px]' : 'h-[48px] w-full'
          )}
        />
      )}

      {/* Left spacer (horizontal top position only) */}
      {!isVertical && isTopPosition && (
        <div className="bg-background-primary rounded-lg self-stretch w-[160px] flex-shrink-0" />
      )}

      {/* Navigation items */}
      {isVertical ? (
        <div className="flex-1 min-h-0 flex flex-col gap-[2px]">
          {visibleItems.map((item, index) => {
            const Icon = item.icon;
            const active = isActive(item.path);
            const isDragging = drag.draggedItem === item.id;
            const isDragOver = drag.dragOverItem === item.id;
            const isChatItem = item.id === 'chat';

            return (
              <motion.div
                key={item.id}
                draggable
                onDragStart={(e) => drag.onDragStart(e as unknown as React.DragEvent, item.id)}
                onDragOver={(e) => drag.onDragOver(e as unknown as React.DragEvent, item.id)}
                onDrop={(e) => drag.onDrop(e as unknown as React.DragEvent, item.id)}
                onDragEnd={drag.onDragEnd}
                initial={{ opacity: 0 }}
                animate={{ opacity: isDragging ? 0.5 : 1 }}
                transition={{ duration: 0.15, delay: index * 0.02 }}
                className={cn(
                  'relative cursor-move group',
                  isCondensedIconOnly ? 'flex-shrink-0' : 'w-full flex-shrink-0',
                  isDragOver && 'ring-2 ring-blue-500 rounded-lg',
                  isChatItem && !isCondensedIconOnly && 'overflow-visible'
                )}
              >
                <div
                  className={cn(
                    'flex flex-col',
                    isCondensedIconOnly ? 'items-start' : 'w-full',
                    isChatItem && !isCondensedIconOnly && 'overflow-visible'
                  )}
                >
                  {/* Chat item with dropdown in icon-only mode */}
                  {isChatItem && isCondensedIconOnly ? (
                    <DropdownMenu open={chatPopoverOpen} onOpenChange={setChatPopoverOpen}>
                      <DropdownMenuTrigger asChild>
                        <button
                          className={cn(
                            'flex items-center justify-center',
                            'rounded-lg transition-colors duration-200 no-drag',
                            'p-2.5',
                            active
                              ? 'bg-background-inverse text-text-inverse'
                              : 'bg-background-primary hover:bg-background-tertiary'
                          )}
                        >
                          <Icon className="w-5 h-5" />
                        </button>
                      </DropdownMenuTrigger>
                      <ChatSessionsDropdown
                        sessions={recentSessions}
                        activeSessionId={activeSessionId}
                        side={navigationPosition === 'left' ? 'right' : 'left'}
                        getSessionStatus={getSessionStatus}
                        clearUnread={clearUnread}
                        onNewChat={onNewChat}
                        onSessionClick={onSessionClick}
                        onShowAll={() => onNavClick('/sessions')}
                      />
                    </DropdownMenu>
                  ) : (
                    <>
                      {isChatItem && !isCondensedIconOnly ? (
                        <div className="relative">
                          <motion.button
                            onClick={onToggleChatExpanded}
                            whileHover={{ scale: 1.02 }}
                            whileTap={{ scale: 0.98 }}
                            className={cn(
                              'flex flex-row items-center gap-2 outline-none',
                              'relative rounded-lg transition-colors duration-200 no-drag',
                              'w-full pl-2 pr-4 py-2.5',
                              active
                                ? 'bg-background-inverse text-text-inverse'
                                : 'bg-background-primary hover:bg-background-tertiary'
                            )}
                          >
                            <div className="opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0">
                              <GripVertical className="w-4 h-4 text-text-secondary" />
                            </div>
                            <Icon className="w-5 h-5 flex-shrink-0" />
                            <span className="text-sm font-medium text-left flex-1">
                              {item.label}
                            </span>
                            <div className="flex-shrink-0">
                              {isChatExpanded ? (
                                <ChevronDown className="w-3 h-3 text-text-secondary" />
                              ) : (
                                <ChevronRight className="w-3 h-3 text-text-secondary" />
                              )}
                            </div>
                          </motion.button>
                          {!isChatExpanded && (
                            <motion.button
                              onClick={(e) => {
                                e.stopPropagation();
                                onNewChat();
                              }}
                              whileHover={{ scale: 1.1 }}
                              whileTap={{ scale: 0.95 }}
                              className={cn(
                                'absolute -right-9 top-1/2 -translate-y-1/2 p-1.5 rounded-md z-10',
                                'opacity-0 group-hover:opacity-100 transition-opacity',
                                'bg-background-tertiary hover:bg-background-inverse hover:text-text-inverse',
                                'flex items-center justify-center'
                              )}
                              title={intl.formatMessage(i18n.newChat)}
                            >
                              <Plus className="w-4 h-4" />
                            </motion.button>
                          )}
                        </div>
                      ) : (
                        <motion.button
                          onClick={() => onNavClick(item.path)}
                          whileHover={{ scale: 1.02 }}
                          whileTap={{ scale: 0.98 }}
                          className={cn(
                            'flex flex-row items-center gap-2',
                            'relative rounded-lg transition-colors duration-200 no-drag',
                            isCondensedIconOnly
                              ? 'justify-center p-2.5'
                              : 'w-full pl-2 pr-4 py-2.5',
                            active
                              ? 'bg-background-inverse text-text-inverse'
                              : 'bg-background-primary hover:bg-background-tertiary'
                          )}
                        >
                          {!isCondensedIconOnly && (
                            <div className="opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0">
                              <GripVertical className="w-4 h-4 text-text-secondary" />
                            </div>
                          )}
                          <Icon className="w-5 h-5 flex-shrink-0" />
                          {!isCondensedIconOnly && (
                            <span className="text-sm font-medium text-left flex-1">
                              {item.label}
                            </span>
                          )}
                          {!isCondensedIconOnly && item.getTag && (
                            <div className="flex items-center gap-1 flex-shrink-0">
                              <span
                                className={cn(
                                  'text-xs font-mono px-2 py-0.5 rounded-full',
                                  active
                                    ? 'bg-background-primary/20 text-text-inverse/80'
                                    : 'bg-background-secondary text-text-secondary'
                                )}
                              >
                                {item.getTag()}
                              </span>
                            </div>
                          )}
                        </motion.button>
                      )}
                    </>
                  )}
                  {isChatItem && !isCondensedIconOnly && (
                    <SessionsList
                      sessions={recentSessions}
                      activeSessionId={activeSessionId}
                      isExpanded={isChatExpanded}
                      getSessionStatus={getSessionStatus}
                      clearUnread={clearUnread}
                      onSessionClick={onSessionClick}
                      onSessionRenamed={onFetchSessions}
                      onNewChat={onNewChat}
                      onShowAll={() => onNavClick('/sessions')}
                    />
                  )}
                </div>
              </motion.div>
            );
          })}

          <div
            className={cn(
              'bg-background-primary rounded-lg flex-1 min-h-[40px]',
              isCondensedIconOnly ? 'w-[40px]' : 'w-full'
            )}
          />
        </div>
      ) : (
        /* Horizontal navigation items */
        visibleItems.map((item, index) => {
          const Icon = item.icon;
          const active = isActive(item.path);
          const isDragging = drag.draggedItem === item.id;
          const isDragOver = drag.dragOverItem === item.id;
          const isChatItem = item.id === 'chat';

          return (
            <motion.div
              key={item.id}
              draggable
              onDragStart={(e) => drag.onDragStart(e as unknown as React.DragEvent, item.id)}
              onDragOver={(e) => drag.onDragOver(e as unknown as React.DragEvent, item.id)}
              onDrop={(e) => drag.onDrop(e as unknown as React.DragEvent, item.id)}
              onDragEnd={drag.onDragEnd}
              initial={{ opacity: 0 }}
              animate={{ opacity: isDragging ? 0.5 : 1 }}
              transition={{ duration: 0.15, delay: index * 0.02 }}
              className={cn(
                'relative cursor-move group flex-shrink-0',
                isDragOver && 'ring-2 ring-blue-500 rounded-lg',
                isChatItem && !isCondensedIconOnly && 'overflow-visible'
              )}
            >
              <div className="flex flex-col">
                {isChatItem ? (
                  <DropdownMenu open={chatPopoverOpen} onOpenChange={setChatPopoverOpen}>
                    <DropdownMenuTrigger asChild>
                      <motion.button
                        whileHover={{ scale: 1.02 }}
                        whileTap={{ scale: 0.98 }}
                        className={cn(
                          'flex flex-row items-center justify-center gap-2',
                          'relative rounded-lg transition-colors duration-200 no-drag',
                          'px-3 py-2.5',
                          active
                            ? 'bg-background-inverse text-text-inverse'
                            : 'bg-background-primary hover:bg-background-tertiary'
                        )}
                      >
                        <Icon className="w-5 h-5 flex-shrink-0" />
                        <span className="text-sm font-medium text-left hidden min-[1200px]:block">
                          {item.label}
                        </span>
                      </motion.button>
                    </DropdownMenuTrigger>
                    <ChatSessionsDropdown
                      sessions={recentSessions}
                      activeSessionId={activeSessionId}
                      side={isTopPosition ? 'bottom' : 'top'}
                      getSessionStatus={getSessionStatus}
                      clearUnread={clearUnread}
                      onNewChat={onNewChat}
                      onSessionClick={onSessionClick}
                      onShowAll={() => onNavClick('/sessions')}
                    />
                  </DropdownMenu>
                ) : (
                  <motion.button
                    onClick={() => onNavClick(item.path)}
                    whileHover={{ scale: 1.02 }}
                    whileTap={{ scale: 0.98 }}
                    className={cn(
                      'flex flex-row items-center gap-2 px-3 py-2.5',
                      'relative rounded-lg transition-colors duration-200 no-drag',
                      active
                        ? 'bg-background-inverse text-text-inverse'
                        : 'bg-background-primary hover:bg-background-tertiary'
                    )}
                  >
                    <Icon className="w-5 h-5 flex-shrink-0" />
                    <span className="text-sm font-medium text-left hidden min-[1200px]:block">
                      {item.label}
                    </span>
                  </motion.button>
                )}
              </div>
            </motion.div>
          );
        })
      )}

      {/* Right spacer (horizontal only) */}
      {!isVertical && (
        <div
          className="bg-background-primary rounded-lg self-stretch flex-1 min-w-[40px]"
          style={
            !isOverlayMode && isTopPosition
              ? ({ WebkitAppRegion: 'drag' } as React.CSSProperties)
              : undefined
          }
        />
      )}
    </motion.div>
  );
};
