import * as React from 'react';
import * as ScrollAreaPrimitive from '@radix-ui/react-scroll-area';

type ScrollBehavior = 'auto' | 'smooth';

import { cn } from '../../utils';

export interface ScrollAreaHandle {
  scrollToBottom: () => void;
  scrollToPosition: (options: { top: number; behavior?: ScrollBehavior }) => void;
  isAtBottom: () => boolean;
  isFollowing: boolean;
  viewportRef: React.RefObject<HTMLDivElement | null>;
}

interface ScrollAreaProps extends React.ComponentPropsWithoutRef<typeof ScrollAreaPrimitive.Root> {
  autoScroll?: boolean;
  onScrollChange?: (isAtBottom: boolean) => void;
  /* padding needs to be passed into the container inside ScrollArea to avoid pushing the scrollbar out */
  paddingX?: number;
  paddingY?: number;
  handleScroll?: (viewport: HTMLDivElement) => void;
}

const ScrollArea = React.forwardRef<ScrollAreaHandle, ScrollAreaProps>(
  (
    {
      className,
      children,
      autoScroll = false,
      onScrollChange,
      paddingX,
      paddingY,
      handleScroll: handleScrollProp,
      ...props
    },
    ref
  ) => {
    const rootRef = React.useRef<React.ElementRef<typeof ScrollAreaPrimitive.Root>>(null);
    const viewportRef = React.useRef<HTMLDivElement>(null);
    const viewportEndRef = React.useRef<HTMLDivElement>(null);
    const contentRef = React.useRef<HTMLDivElement>(null);
    const [isFollowing, setIsFollowing] = React.useState(true);
    const [isScrolled, setIsScrolled] = React.useState(false);
    const userScrolledUpRef = React.useRef(false);
    const lastScrollHeightRef = React.useRef(0);
    const lastScrollTopRef = React.useRef(0);
    const pendingGrowthRef = React.useRef(false);
    const pinFrameRef = React.useRef<number | null>(null);
    const autoScrollRef = React.useRef(autoScroll);
    const isActivelyScrollingRef = React.useRef(false);
    const scrollTimeoutRef = React.useRef<number | null>(null);

    React.useLayoutEffect(() => {
      autoScrollRef.current = autoScroll;
    }, [autoScroll]);

    const BOTTOM_SCROLL_THRESHOLD = 200;

    const isAtBottom = React.useCallback(() => {
      if (!viewportRef.current) return false;

      const viewport = viewportRef.current;
      const { scrollHeight, scrollTop, clientHeight } = viewport;
      const distanceFromBottom = scrollHeight - scrollTop - clientHeight;

      return distanceFromBottom <= BOTTOM_SCROLL_THRESHOLD;
    }, []);

    const pinToBottom = React.useCallback(() => {
      const viewport = viewportRef.current;
      if (!viewport) return;
      viewport.scrollTo({ top: viewport.scrollHeight, behavior: 'auto' });
      // The native scroll event may arrive after another content resize.
      lastScrollTopRef.current = viewport.scrollTop;
      lastScrollHeightRef.current = viewport.scrollHeight;
      pendingGrowthRef.current = false;
    }, []);

    const queuePin = React.useCallback(() => {
      if (
        !autoScrollRef.current ||
        userScrolledUpRef.current ||
        isActivelyScrollingRef.current ||
        !pendingGrowthRef.current ||
        pinFrameRef.current !== null
      ) {
        return;
      }
      pinFrameRef.current = requestAnimationFrame(() => {
        pinFrameRef.current = null;
        // Reading intent can change between the resize and this frame.
        if (
          autoScrollRef.current &&
          !userScrolledUpRef.current &&
          !isActivelyScrollingRef.current &&
          pendingGrowthRef.current
        ) {
          pinToBottom();
        }
      });
    }, [pinToBottom]);

    const checkForGrowth = React.useCallback(() => {
      const viewport = viewportRef.current;
      if (!viewport || !autoScrollRef.current) return;
      if (viewport.scrollHeight > lastScrollHeightRef.current && !userScrolledUpRef.current) {
        pendingGrowthRef.current = true;
      }
      lastScrollHeightRef.current = viewport.scrollHeight;
      queuePin();
    }, [queuePin]);

    const scrollToBottom = React.useCallback(() => {
      if (viewportRef.current) {
        if (scrollTimeoutRef.current !== null) clearTimeout(scrollTimeoutRef.current);
        scrollTimeoutRef.current = null;
        isActivelyScrollingRef.current = false;
        pinToBottom();
        setIsFollowing(true);
        userScrolledUpRef.current = false;
        onScrollChange?.(true);
      }
    }, [onScrollChange, pinToBottom]);

    const scrollToPosition = React.useCallback(
      ({ top, behavior = 'smooth' }: { top: number; behavior?: ScrollBehavior }) => {
        if (viewportRef.current) {
          viewportRef.current.scrollTo({
            top,
            behavior,
          });
        }
      },
      []
    );

    // Expose the scroll methods to parent components
    React.useImperativeHandle(
      ref,
      () => ({
        scrollToBottom,
        scrollToPosition,
        isAtBottom,
        isFollowing,
        viewportRef,
      }),
      [scrollToBottom, scrollToPosition, isAtBottom, isFollowing]
    );

    const handleScroll = React.useCallback(() => {
      if (!viewportRef.current) return;

      const viewport = viewportRef.current;
      const { scrollTop } = viewport;
      const currentIsAtBottom = isAtBottom();

      const isScrollingUp = scrollTop < lastScrollTopRef.current;
      const scrollDelta = Math.abs(scrollTop - lastScrollTopRef.current);
      if (scrollDelta > 0) {
        isActivelyScrollingRef.current = true;
        if (scrollTimeoutRef.current !== null) {
          clearTimeout(scrollTimeoutRef.current);
        }
        scrollTimeoutRef.current = window.setTimeout(() => {
          scrollTimeoutRef.current = null;
          isActivelyScrollingRef.current = false;
          queuePin();
        }, 100);
      }

      lastScrollTopRef.current = scrollTop;

      // Growth alone can move the bottom away without any upward scrolling.
      if (isScrollingUp && !currentIsAtBottom && !userScrolledUpRef.current) {
        userScrolledUpRef.current = true;
        pendingGrowthRef.current = false;
        setIsFollowing(false);
        onScrollChange?.(false);
      } else if (currentIsAtBottom && userScrolledUpRef.current) {
        userScrolledUpRef.current = false;
        setIsFollowing(true);
        onScrollChange?.(true);
      }

      checkForGrowth();
      setIsScrolled(scrollTop > 0);

      if (handleScrollProp) {
        handleScrollProp(viewport);
      }
    }, [isAtBottom, onScrollChange, handleScrollProp, queuePin, checkForGrowth]);

    // Auto-scroll when content changes and user is following
    React.useEffect(() => {
      if (autoScroll) checkForGrowth();
    }, [children, autoScroll, isFollowing, checkForGrowth]);

    // Keep pinned to the bottom when content grows from async media (images,
    // syntax highlighting) that resizes after paint without a React re-render,
    // so it isn't covered by the [children] effect above.
    React.useEffect(() => {
      if (!autoScroll) return;
      const content = contentRef.current;
      if (!content || typeof ResizeObserver === 'undefined') return;
      const observer = new ResizeObserver(checkForGrowth);
      observer.observe(content);
      return () => observer.disconnect();
    }, [autoScroll, checkForGrowth]);

    // Add scroll event listener
    React.useEffect(() => {
      const viewport = viewportRef.current;
      if (!viewport) return;

      viewport.addEventListener('scroll', handleScroll, { passive: true });
      return () => viewport.removeEventListener('scroll', handleScroll);
    }, [handleScroll]);

    React.useEffect(() => {
      return () => {
        if (pinFrameRef.current !== null) cancelAnimationFrame(pinFrameRef.current);
        if (scrollTimeoutRef.current !== null) clearTimeout(scrollTimeoutRef.current);
        pinFrameRef.current = null;
        scrollTimeoutRef.current = null;
        isActivelyScrollingRef.current = false;
        // Keep pending growth so StrictMode's effect replay can queue it again.
      };
    }, [autoScroll]);

    return (
      <ScrollAreaPrimitive.Root
        ref={rootRef}
        className={cn('relative overflow-hidden', className)}
        data-scrolled={isScrolled}
        {...props}
      >
        <div className={cn('absolute top-0 left-0 right-0 z-10 transition-all duration-200')} />
        <ScrollAreaPrimitive.Viewport
          ref={viewportRef}
          className="h-full w-full rounded-[inherit] [&>div]:!block"
        >
          <div
            ref={contentRef}
            className={cn(paddingX ? `px-${paddingX}` : '', paddingY ? `py-${paddingY}` : '')}
          >
            {children}
            {autoScroll && <div ref={viewportEndRef} style={{ height: '1px' }} />}
          </div>
        </ScrollAreaPrimitive.Viewport>
        <ScrollBar />
        <ScrollAreaPrimitive.Corner />
      </ScrollAreaPrimitive.Root>
    );
  }
);
ScrollArea.displayName = ScrollAreaPrimitive.Root.displayName;

const ScrollBar = React.forwardRef<
  React.ElementRef<typeof ScrollAreaPrimitive.ScrollAreaScrollbar>,
  React.ComponentPropsWithoutRef<typeof ScrollAreaPrimitive.ScrollAreaScrollbar>
>(({ className, orientation = 'vertical', ...props }, ref) => (
  <ScrollAreaPrimitive.ScrollAreaScrollbar
    ref={ref}
    orientation={orientation}
    className={cn(
      'flex touch-none select-none transition-colors',
      orientation === 'vertical' && 'h-full w-2.5 border-l border-l-transparent p-[1px]',
      orientation === 'horizontal' && 'h-2.5 flex-col border-t border-t-transparent p-[1px]',
      className
    )}
    {...props}
  >
    <ScrollAreaPrimitive.ScrollAreaThumb className="relative flex-1 rounded-full bg-border-primary dark:bg-background-secondary" />
  </ScrollAreaPrimitive.ScrollAreaScrollbar>
));
ScrollBar.displayName = ScrollAreaPrimitive.ScrollAreaScrollbar.displayName;

export { ScrollArea, ScrollBar };
