import React from 'react';
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, waitFor } from '@testing-library/react';

import { useSessionWorkingDir } from './useSessionWorkingDir';
import * as api from '../api';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function HookHarness({
  sessionId,
  onValue,
}: {
  sessionId: string | null;
  onValue: (value: string | null) => void;
}) {
  const { sessionWorkingDir } = useSessionWorkingDir(sessionId);

  React.useEffect(() => {
    onValue(sessionWorkingDir);
  }, [sessionWorkingDir, onValue]);

  return null;
}

describe('useSessionWorkingDir', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('clears working dir when sessionId is null', async () => {
    const seen: Array<string | null> = [];

    render(<HookHarness sessionId={null} onValue={(v) => seen.push(v)} />);

    await waitFor(() => {
      expect(seen[seen.length - 1]).toBeNull();
    });
  });

  it('ignores stale out-of-order response when switching sessions', async () => {
    const a = deferred<{ data?: { working_dir?: string } }>();
    const b = deferred<{ data?: { working_dir?: string } }>();

    const getSessionMock = vi
      .spyOn(api, 'getSession')
      .mockImplementation(({ path }: { path: { session_id: string } }) => {
        if (path.session_id === 'A') return a.promise as ReturnType<typeof api.getSession>;
        if (path.session_id === 'B') return b.promise as ReturnType<typeof api.getSession>;
        return Promise.resolve({ data: { working_dir: '/unknown' } }) as ReturnType<
          typeof api.getSession
        >;
      });

    const seen: Array<string | null> = [];

    const { rerender } = render(<HookHarness sessionId={'A'} onValue={(v) => seen.push(v)} />);

    // Switch quickly to B before A resolves.
    rerender(<HookHarness sessionId={'B'} onValue={(v) => seen.push(v)} />);

    // Resolve B first (current session)
    b.resolve({ data: { working_dir: '/path/B' } });

    await waitFor(() => {
      expect(seen[seen.length - 1]).toBe('/path/B');
    });

    // Resolve A late (stale); should be ignored.
    a.resolve({ data: { working_dir: '/path/A' } });

    // Give microtasks a chance to flush.
    await Promise.resolve();
    await Promise.resolve();

    expect(seen[seen.length - 1]).toBe('/path/B');
    expect(getSessionMock).toHaveBeenCalledTimes(2);
  });
});
