import type { UserInput } from '../types/message';

export type DeferredSubmitDisposition = 'started' | 'dropped' | 'busy';

const deferredSubmitsBySession = new Map<string, UserInput[]>();
const drainInFlight = new Set<string>();

export function enqueueDeferredSubmit(sessionId: string, input: UserInput): void {
  const queued = deferredSubmitsBySession.get(sessionId);
  if (queued) {
    queued.push(input);
    return;
  }
  deferredSubmitsBySession.set(sessionId, [input]);
}

export function clearDeferredSubmits(): void {
  deferredSubmitsBySession.clear();
  drainInFlight.clear();
}

export function drainDeferredSubmit(
  sessionId: string,
  canAccept: () => boolean,
  submitHead: (input: UserInput) => DeferredSubmitDisposition
): void {
  if (drainInFlight.has(sessionId)) return;
  const queued = deferredSubmitsBySession.get(sessionId);
  if (!queued?.length || !canAccept()) return;

  drainInFlight.add(sessionId);
  const input = queued[0];
  let disposition: DeferredSubmitDisposition;
  try {
    disposition = submitHead(input);
  } catch {
    return;
  } finally {
    drainInFlight.delete(sessionId);
  }

  if (disposition === 'busy') return;

  if (queued[0] === input) {
    queued.shift();
    if (queued.length === 0) deferredSubmitsBySession.delete(sessionId);
  }

  if (disposition === 'dropped') {
    drainDeferredSubmit(sessionId, canAccept, submitHead);
  }
}
