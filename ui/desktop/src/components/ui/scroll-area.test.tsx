import { createRef, StrictMode, useLayoutEffect } from 'react';
import { act, fireEvent, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ScrollArea, type ScrollAreaHandle } from './scroll-area';

type ResizeCallback = ConstructorParameters<typeof ResizeObserver>[0];
type FrameCallback = Parameters<typeof requestAnimationFrame>[0];

// jsdom has no layout. Keep resize notifications and scroll-event delivery
// separate so a result can grow before a previously queued event arrives.
class ResizeObserverStub {
  static instances: ResizeObserverStub[] = [];
  targets = new Set<Element>();

  constructor(public callback: ResizeCallback) {
    ResizeObserverStub.instances.push(this);
  }

  observe(target: Element) {
    this.targets.add(target);
  }
  unobserve(target: Element) {
    this.targets.delete(target);
  }
  disconnect() {
    this.targets.clear();
  }
}

const frames = new Map<number, FrameCallback>();
let nextFrame = 0;

function flushFrames() {
  act(() => {
    const pending = [...frames.values()];
    frames.clear();
    pending.forEach((callback) => callback(0));
  });
}

function resize(content: Element) {
  act(() => {
    for (const observer of ResizeObserverStub.instances) {
      if (observer.targets.has(content)) {
        observer.callback([], observer as unknown as ResizeObserver);
      }
    }
  });
}

function setHeight(viewport: HTMLDivElement, height: number) {
  Object.defineProperty(viewport, 'scrollHeight', { value: height, configurable: true });
}

function layOut(viewport: HTMLDivElement) {
  Object.defineProperty(viewport, 'clientHeight', { value: 400, configurable: true });
  setHeight(viewport, 1000);
  viewport.scrollTo = vi.fn((options?: { top?: number } | number, y?: number) => {
    const top = typeof options === 'number' ? y : options?.top;
    viewport.scrollTop = Math.max(0, Math.min(top ?? 0, viewport.scrollHeight - 400));
  });
}

function scrollTo(viewport: HTMLDivElement, top: number) {
  viewport.scrollTop = top;
  fireEvent.scroll(viewport);
}

function settle() {
  act(() => vi.advanceTimersByTime(150));
  flushFrames();
}

function renderScrollArea(autoScroll = true) {
  const ref = createRef<ScrollAreaHandle>();
  const child = <div data-testid="content">Message</div>;
  const result = render(
    <ScrollArea autoScroll={autoScroll} ref={ref}>
      {child}
    </ScrollArea>
  );
  const viewport = ref.current!.viewportRef.current!;
  const content = result.getByTestId('content').parentElement!;
  layOut(viewport);
  act(() => ref.current!.scrollToBottom());
  fireEvent.scroll(viewport);
  settle();
  vi.mocked(viewport.scrollTo).mockClear();
  return { ...result, ref, viewport, content, child };
}

beforeEach(() => {
  vi.useFakeTimers();
  ResizeObserverStub.instances = [];
  vi.stubGlobal('ResizeObserver', ResizeObserverStub);
  frames.clear();
  nextFrame = 0;
  vi.stubGlobal('requestAnimationFrame', (callback: FrameCallback) => {
    frames.set(++nextFrame, callback);
    return nextFrame;
  });
  vi.stubGlobal('cancelAnimationFrame', (id: number) => frames.delete(id));
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe('ScrollArea auto-follow', () => {
  it.each([180, 260])('follows %ipx growth before an internal scroll event arrives', (growth) => {
    const { ref, viewport, content } = renderScrollArea();
    setHeight(viewport, 1040);
    act(() => ref.current!.scrollToBottom());
    setHeight(viewport, 1040 + growth);
    fireEvent.scroll(viewport);
    resize(content);
    settle();

    expect(ref.current!.isFollowing).toBe(true);
    expect(viewport.scrollTop).toBe(640 + growth);

    setHeight(viewport, 1080 + growth);
    resize(content);
    settle();
    expect(viewport.scrollTop).toBe(680 + growth);
  });

  it('follows asynchronous resizing without a React render or scroll event', () => {
    const { ref, viewport, content } = renderScrollArea();
    setHeight(viewport, 1260);
    resize(content);
    settle();
    expect(ref.current!.isFollowing).toBe(true);
    expect(viewport.scrollTop).toBe(860);
  });

  it('retries growth during the scroll pause without another resize or message', () => {
    const { ref, viewport, content } = renderScrollArea();
    scrollTo(viewport, 590);
    act(() => vi.advanceTimersByTime(35));
    setHeight(viewport, 1260);
    resize(content);
    flushFrames();
    expect(viewport.scrollTop).toBe(590);

    settle();
    expect(ref.current!.isFollowing).toBe(true);
    expect(viewport.scrollTop).toBe(860);
  });

  it('follows new growth after scrolling up and returning to the bottom', () => {
    const { ref, viewport, content } = renderScrollArea();
    scrollTo(viewport, 300);
    expect(ref.current!.isFollowing).toBe(false);
    settle();
    scrollTo(viewport, 600);
    expect(ref.current!.isFollowing).toBe(true);
    setHeight(viewport, 1260);
    resize(content);
    settle();
    expect(viewport.scrollTop).toBe(860);
  });

  it('preserves the 200px bottom threshold when pausing and resuming', () => {
    const { ref, viewport } = renderScrollArea();
    scrollTo(viewport, 400);
    expect(ref.current!.isAtBottom()).toBe(true);
    expect(ref.current!.isFollowing).toBe(true);
    scrollTo(viewport, 399);
    expect(ref.current!.isAtBottom()).toBe(false);
    expect(ref.current!.isFollowing).toBe(false);
    scrollTo(viewport, 400);
    expect(ref.current!.isFollowing).toBe(true);
  });

  it('does not follow asynchronous growth while reading older messages', () => {
    const { ref, viewport, content } = renderScrollArea();
    scrollTo(viewport, 300);
    settle();
    setHeight(viewport, 1260);
    resize(content);
    settle();
    expect(ref.current!.isFollowing).toBe(false);
    expect(viewport.scrollTop).toBe(300);
    expect(viewport.scrollTo).not.toHaveBeenCalled();
  });

  it('rechecks following before executing an already queued frame', () => {
    const { ref, viewport, content } = renderScrollArea();
    setHeight(viewport, 1260);
    resize(content);
    scrollTo(viewport, 300);
    // Let the active-scroll guard expire before the queued frame runs.
    settle();
    expect(ref.current!.isFollowing).toBe(false);
    expect(viewport.scrollTo).not.toHaveBeenCalled();
  });

  it('rechecks following in a frame queued by the retry timer', () => {
    const { ref, viewport, content } = renderScrollArea();
    scrollTo(viewport, 590);
    setHeight(viewport, 1260);
    resize(content);
    act(() => vi.advanceTimersByTime(100));
    scrollTo(viewport, 300);
    settle();
    expect(ref.current!.isFollowing).toBe(false);
    expect(viewport.scrollTo).not.toHaveBeenCalled();
  });

  it('keeps a deferred resize retry when the scroll callback changes', () => {
    const { ref, viewport, content, child, rerender } = renderScrollArea();
    scrollTo(viewport, 590);
    setHeight(viewport, 1260);
    resize(content);
    rerender(
      <ScrollArea autoScroll ref={ref} onScrollChange={vi.fn()}>
        {child}
      </ScrollArea>
    );
    settle();
    expect(viewport.scrollTop).toBe(860);
  });

  it('does not interpret a content-shrink clamp as leaving the bottom', () => {
    const { ref, viewport } = renderScrollArea();
    setHeight(viewport, 900);
    scrollTo(viewport, 500);
    expect(ref.current!.isFollowing).toBe(true);
  });

  it('detects an upward scroll before an internal pin event arrives', () => {
    const { ref, viewport } = renderScrollArea();
    setHeight(viewport, 1260);
    act(() => ref.current!.scrollToBottom());
    scrollTo(viewport, 599);
    expect(ref.current!.isFollowing).toBe(false);
  });

  it('cancels queued scrolling when autoScroll is disabled', () => {
    const { ref, viewport, content, child, rerender } = renderScrollArea();
    setHeight(viewport, 1260);
    resize(content);
    rerender(<ScrollArea ref={ref}>{child}</ScrollArea>);
    settle();
    expect(viewport.scrollTo).not.toHaveBeenCalled();

    rerender(
      <ScrollArea autoScroll ref={ref}>
        {child}
      </ScrollArea>
    );
    settle();
    expect(viewport.scrollTop).toBe(860);
  });

  it('keeps explicit navigation available when autoScroll is disabled', () => {
    const { ref, viewport, content } = renderScrollArea(false);
    setHeight(viewport, 1260);
    resize(content);
    settle();
    expect(viewport.scrollTo).not.toHaveBeenCalled();
    act(() => ref.current!.scrollToBottom());
    expect(viewport.scrollTop).toBe(860);
  });

  it.each(['frame', 'timer'])('cancels a pending %s on unmount', (pending) => {
    const { viewport, content, unmount } = renderScrollArea();
    if (pending === 'timer') scrollTo(viewport, 590);
    setHeight(viewport, 1260);
    resize(content);
    unmount();
    settle();
    expect(viewport.scrollTo).not.toHaveBeenCalled();
    expect(frames.size).toBe(0);
  });

  it('follows initial content after StrictMode effect replay', () => {
    const ref = createRef<ScrollAreaHandle>();
    function InitiallyLaidOutScrollArea() {
      useLayoutEffect(() => layOut(ref.current!.viewportRef.current!), []);
      return (
        <ScrollArea autoScroll ref={ref}>
          Message
        </ScrollArea>
      );
    }
    render(
      <StrictMode>
        <InitiallyLaidOutScrollArea />
      </StrictMode>
    );
    flushFrames();
    expect(ref.current!.viewportRef.current!.scrollTop).toBe(600);
  });
});
