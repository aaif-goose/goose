import React, { useState, useEffect, useRef } from 'react';
import { GripVertical } from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import { Z_INDEX } from './constants';
import { cn } from '../../utils';
import { DropdownMenu, DropdownMenuTrigger } from '../ui/dropdown-menu';
import { ChatSessionsDropdown } from './navigation';
import { ChatHistorySearch } from '../conversation/ChatHistorySearch';
import type { NavigationRendererProps } from './navigation/types';

export const ExpandedRenderer: React.FC<NavigationRendererProps> = ({
  isNavExpanded,
  isOverlayMode,
  navigationPosition,
  onClose,
  className,
  visibleItems,
  isActive,
  recentSessions,
  activeSessionId,
  onNavClick,
  onNewChat,
  onSessionClick,
  getSessionStatus,
  clearUnread,
  drag,
  navFocusRef,
}) => {
  const [chatDropdownOpen, setChatDropdownOpen] = useState(false);
  const [gridColumns, setGridColumns] = useState(2);
  const [gridMeasured, setGridMeasured] = useState(false);
  const [tilesReady, setTilesReady] = useState(false);
  const [isClosing, setIsClosing] = useState(false);
  const prevIsNavExpandedRef = useRef(isNavExpanded);
  const gridRef = useRef<HTMLDivElement>(null);

  // Detect when nav is closing
  useEffect(() => {
    if (prevIsNavExpandedRef.current && !isNavExpanded) {
      setIsClosing(true);
      setTilesReady(false);
    } else if (!prevIsNavExpandedRef.current && isNavExpanded) {
      setIsClosing(false);
    }
    prevIsNavExpandedRef.current = isNavExpanded;
  }, [isNavExpanded]);

  // Delay tiles animation until panel opens
  useEffect(() => {
    if (!isNavExpanded) {
      setTilesReady(false);
      return;
    }
    const timeoutId = setTimeout(() => setTilesReady(true), 150);
    return () => clearTimeout(timeoutId);
  }, [isNavExpanded]);

  // Track grid columns for spacer tiles
  useEffect(() => {
    if (!isNavExpanded) {
      setGridMeasured(false);
      return;
    }

    setGridMeasured(false);
    let rafId: number;

    const updateGridColumns = () => {
      if (!gridRef.current) return;
      const parent = gridRef.current.parentElement;
      if (!parent) return;

      const parentStyle = window.getComputedStyle(parent);
      const availableWidth =
        parent.clientWidth -
        parseFloat(parentStyle.paddingLeft) -
        parseFloat(parentStyle.paddingRight);

      const minSize = navigationPosition === 'left' || navigationPosition === 'right' ? 140 : 160;
      const gap = isOverlayMode ? 12 : 2;
      const cols = Math.max(1, Math.floor((availableWidth + gap) / (minSize + gap)));

      setGridColumns(cols);
      setGridMeasured(true);
    };

    const timeoutId = setTimeout(() => {
      rafId = requestAnimationFrame(updateGridColumns);
    }, 100);

    const resizeObserver = new ResizeObserver(() => {
      cancelAnimationFrame(rafId);
      rafId = requestAnimationFrame(updateGridColumns);
    });

    const parent = gridRef.current?.parentElement;
    if (parent) resizeObserver.observe(parent);

    return () => {
      clearTimeout(timeoutId);
      cancelAnimationFrame(rafId);
      resizeObserver.disconnect();
    };
  }, [isNavExpanded, navigationPosition, isOverlayMode]);

  const isPushTopNav = !isOverlayMode && navigationPosition === 'top';
  const dragStyle = isPushTopNav ? ({ WebkitAppRegion: 'drag' } as React.CSSProperties) : undefined;
  const showContent = !isClosing || isOverlayMode;

  const navContent = (
    <motion.div
      ref={navFocusRef}
      tabIndex={-1}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.15 }}
      className={cn(
        'bg-app h-full overflow-hidden outline-none',
        isOverlayMode && 'backdrop-blur-md shadow-2xl rounded-lg p-4',
        !isOverlayMode && navigationPosition === 'top' && 'pb-[2px]',
        !isOverlayMode && navigationPosition === 'bottom' && 'pt-[2px]',
        !isOverlayMode && navigationPosition === 'left' && 'pr-[2px]',
        !isOverlayMode && navigationPosition === 'right' && 'pl-[2px]',
        className
      )}
    >
      {showContent ? (
        <div
          ref={gridRef}
          className={cn(
            'grid gap-[2px] overflow-y-auto overflow-x-hidden h-full',
            isOverlayMode && 'gap-3'
          )}
          style={{
            ...(dragStyle || {}),
            gridTemplateColumns: isOverlayMode
              ? 'repeat(auto-fit, minmax(120px, 1fr))'
              : navigationPosition === 'left' || navigationPosition === 'right'
                ? 'repeat(auto-fit, minmax(140px, 1fr))'
                : 'repeat(auto-fit, minmax(160px, 1fr))',
            alignContent: 'start',
          }}
        >
          {/* Search bar spanning full width */}
          <div
            className="col-span-full p-2"
            style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}
          >
            <ChatHistorySearch
              onSessionClick={onSessionClick}
              getSessionStatus={getSessionStatus}
              clearUnread={clearUnread}
              activeSessionId={activeSessionId}
            />
          </div>

          {visibleItems.map((item, index) => {
            const Icon = item.icon;
            const active = isActive(item.path);
            const isDragging = drag.draggedItem === item.id;
            const isDragOver = drag.dragOverItem === item.id;
            const isChatItem = item.id === 'chat';

            if (isChatItem) {
              return (
                <DropdownMenu
                  key={item.id}
                  open={chatDropdownOpen}
                  onOpenChange={setChatDropdownOpen}
                >
                  <motion.div
                    draggable
                    onDragStart={(e) => drag.onDragStart(e as unknown as React.DragEvent, item.id)}
                    onDragOver={(e) => drag.onDragOver(e as unknown as React.DragEvent, item.id)}
                    onDrop={(e) => drag.onDrop(e as unknown as React.DragEvent, item.id)}
                    onDragEnd={drag.onDragEnd}
                    initial={{ opacity: 0 }}
                    animate={{ opacity: tilesReady ? (isDragging ? 0.5 : 1) : 0 }}
                    transition={{ duration: 0.15, delay: tilesReady ? index * 0.03 : 0 }}
                    className={cn(
                      'relative cursor-move group',
                      isDragOver && 'ring-2 ring-blue-500 rounded-lg'
                    )}
                  >
                    <div className="relative">
                      <DropdownMenuTrigger asChild>
                        <motion.div
                          className={cn(
                            'w-full relative flex flex-col rounded-lg',
                            'transition-colors duration-200 aspect-square cursor-pointer',
                            active
                              ? 'bg-background-inverse text-text-inverse'
                              : 'bg-background-primary hover:bg-background-tertiary'
                          )}
                        >
                          <div className="flex-1 flex flex-col items-start justify-between p-5 no-drag text-left">
                            <div className="absolute top-2 right-2 opacity-0 group-hover:opacity-100 transition-opacity z-10">
                              <GripVertical className="w-4 h-4 text-text-secondary" />
                            </div>
                            {item.getTag && (
                              <div
                                className={cn(
                                  'absolute top-3 px-2 py-1 rounded-full',
                                  item.tagAlign === 'left' ? 'left-8' : 'right-8',
                                  'bg-background-secondary'
                                )}
                              >
                                <span className="text-xs font-mono text-text-secondary">
                                  {item.getTag()}
                                </span>
                              </div>
                            )}
                            <div className="mt-auto w-full">
                              <Icon className="w-6 h-6 mb-2" />
                              <h2 className="font-light text-left text-xl">{item.label}</h2>
                            </div>
                          </div>
                        </motion.div>
                      </DropdownMenuTrigger>
                    </div>
                    <ChatSessionsDropdown
                      sessions={recentSessions}
                      activeSessionId={activeSessionId}
                      side="right"
                      zIndex={Z_INDEX.DROPDOWN_ABOVE_OVERLAY}
                      getSessionStatus={getSessionStatus}
                      clearUnread={clearUnread}
                      onNewChat={onNewChat}
                      onSessionClick={onSessionClick}
                      onShowAll={() => onNavClick('/sessions')}
                    />
                  </motion.div>
                </DropdownMenu>
              );
            }

            return (
              <motion.div
                key={item.id}
                draggable
                onDragStart={(e) => drag.onDragStart(e as unknown as React.DragEvent, item.id)}
                onDragOver={(e) => drag.onDragOver(e as unknown as React.DragEvent, item.id)}
                onDrop={(e) => drag.onDrop(e as unknown as React.DragEvent, item.id)}
                onDragEnd={drag.onDragEnd}
                initial={{ opacity: 0 }}
                animate={{ opacity: tilesReady ? (isDragging ? 0.5 : 1) : 0 }}
                transition={{ duration: 0.15, delay: tilesReady ? index * 0.03 : 0 }}
                className={cn(
                  'relative cursor-move group',
                  isDragOver && 'ring-2 ring-blue-500 rounded-lg'
                )}
              >
                <motion.div
                  className={cn(
                    'w-full relative flex flex-col rounded-lg',
                    'transition-colors duration-200 aspect-square',
                    active
                      ? 'bg-background-inverse text-text-inverse'
                      : 'bg-background-primary hover:bg-background-tertiary'
                  )}
                >
                  <button
                    onClick={() => onNavClick(item.path)}
                    className="flex-1 flex flex-col items-start justify-between p-5 no-drag text-left"
                  >
                    <div className="absolute top-2 right-2 opacity-0 group-hover:opacity-100 transition-opacity z-10">
                      <GripVertical className="w-4 h-4 text-text-secondary" />
                    </div>
                    {item.getTag && (
                      <div
                        className={cn(
                          'absolute top-3 px-2 py-1 rounded-full',
                          item.tagAlign === 'left' ? 'left-8' : 'right-8',
                          'bg-background-secondary'
                        )}
                      >
                        <span className="text-xs font-mono text-text-secondary">
                          {item.getTag()}
                        </span>
                      </div>
                    )}
                    <div className="mt-auto w-full">
                      <Icon className="w-6 h-6 mb-2" />
                      <h2 className="font-light text-left text-xl">{item.label}</h2>
                    </div>
                  </button>
                </motion.div>
              </motion.div>
            );
          })}

          {/* Spacer tiles */}
          {!isOverlayMode &&
            gridMeasured &&
            gridColumns >= 2 &&
            Array.from({
              length:
                navigationPosition === 'left' || navigationPosition === 'right'
                  ? ((gridColumns - (visibleItems.length % gridColumns)) % gridColumns) +
                    gridColumns * 6
                  : (gridColumns - (visibleItems.length % gridColumns)) % gridColumns,
            }).map((_, index) => (
              <div key={`spacer-${index}`} className="relative">
                <div className="w-full aspect-square rounded-lg bg-background-primary" />
              </div>
            ))}
        </div>
      ) : null}
    </motion.div>
  );

  // Expanded overlay uses its own AnimatePresence
  if (isOverlayMode) {
    return (
      <AnimatePresence>
        {isNavExpanded && (
          <div className="fixed inset-0" style={{ zIndex: Z_INDEX.OVERLAY }}>
            <motion.div
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              className="absolute inset-0 bg-black/20 backdrop-blur-sm"
              onClick={onClose}
            />
            <div className="absolute inset-0 overflow-y-auto pointer-events-none">
              <div className="min-h-full flex items-center justify-center p-8">
                <div className="pointer-events-auto max-w-3xl w-full">{navContent}</div>
              </div>
            </div>
          </div>
        )}
      </AnimatePresence>
    );
  }

  return navContent;
};
