import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import { useSessionEvents, type SessionEvent } from './useSessionEvents';

vi.mock('../api', () => ({
  sessionEvents: vi.fn(),
}));

import { sessionEvents } from '../api';

const sessionEventsMock = sessionEvents as unknown as ReturnType<typeof vi.fn>;

function emptyStream() {
  return {
    stream: (async function* () {
      // no events
    })(),
  };
}

function boundedMock(
  limit: number,
  factory: (callIndex: number) => Promise<{ stream: AsyncGenerator<unknown> }>
) {
  let calls = 0;
  return () => {
    const idx = calls++;
    if (idx >= limit) {
      return new Promise(() => {});
    }
    return factory(idx);
  };
}

async function flush(times = 200) {
  for (let i = 0; i < times; i++) {
    await Promise.resolve();
  }
}

describe('useSessionEvents reconnect (issue #8717)', () => {
  let originalSetTimeout: typeof globalThis.setTimeout;

  beforeEach(() => {
    sessionEventsMock.mockReset();
    originalSetTimeout = globalThis.setTimeout;
    globalThis.setTimeout = ((cb: () => void) => {
      globalThis.queueMicrotask(cb);
      return 0 as unknown as ReturnType<typeof globalThis.setTimeout>;
    }) as typeof globalThis.setTimeout;
  });

  afterEach(() => {
    globalThis.setTimeout = originalSetTimeout;
  });

  it('synthesises a terminal Error for active listeners after a sustained failure streak', async () => {
    const realDateNow = Date.now;
    let fakeNow = 1_000_000;
    Date.now = () => fakeNow;

    try {
      sessionEventsMock.mockImplementation(
        boundedMock(60, () => {
          fakeNow += 10_000;
          return Promise.resolve(emptyStream());
        })
      );

      const { result, unmount } = renderHook(() => useSessionEvents('sess-1'));

      const handler = vi.fn();
      act(() => {
        result.current.addListener('req-1', handler);
      });

      await flush();

      const errorCalls = handler.mock.calls.filter(
        (args) => (args[0] as SessionEvent).type === 'Error'
      );
      expect(errorCalls.length).toBeGreaterThanOrEqual(1);

      const firstError = errorCalls[0][0] as SessionEvent & { error: string };
      expect(firstError.error).toBe('Lost connection to server');
      expect(firstError.request_id).toBe('req-1');
      expect(firstError.chat_request_id).toBe('req-1');

      unmount();
    } finally {
      Date.now = realDateNow;
    }
  });
});
