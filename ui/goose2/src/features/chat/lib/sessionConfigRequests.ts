import { acpPrepareSession, acpSetModel } from "@/shared/api/acp";

export interface SessionConfigRequest {
  sessionId: string;
  providerId: string;
  workingDir: string;
  modelId?: string | null;
}

export interface SessionConfigResult {
  applied: boolean;
}

interface QueuedSessionConfigRequest extends SessionConfigRequest {
  sequence: number;
}

interface SessionConfigWaiter {
  sequence: number;
  request: QueuedSessionConfigRequest;
  resolve: (result: SessionConfigResult) => void;
  reject: (error: unknown) => void;
}

interface SessionConfigQueue {
  latest: QueuedSessionConfigRequest | null;
  nextSequence: number;
  running: boolean;
  waiters: SessionConfigWaiter[];
}

const queues = new Map<string, SessionConfigQueue>();

function getQueue(sessionId: string): SessionConfigQueue {
  let queue = queues.get(sessionId);
  if (!queue) {
    queue = {
      latest: null,
      nextSequence: 0,
      running: false,
      waiters: [],
    };
    queues.set(sessionId, queue);
  }
  return queue;
}

function sameSessionConfig(
  a: SessionConfigRequest,
  b: SessionConfigRequest,
): boolean {
  return (
    a.providerId === b.providerId &&
    a.workingDir === b.workingDir &&
    (a.modelId ?? null) === (b.modelId ?? null)
  );
}

function settleFailedWaiters(
  queue: SessionConfigQueue,
  sequence: number,
  error: unknown,
) {
  const remaining: SessionConfigWaiter[] = [];
  for (const waiter of queue.waiters) {
    if (waiter.sequence > sequence) {
      remaining.push(waiter);
      continue;
    }

    if (waiter.sequence === sequence) {
      waiter.reject(error);
    } else {
      waiter.resolve({ applied: false });
    }
  }
  queue.waiters = remaining;
}

function settleAppliedWaiters(
  queue: SessionConfigQueue,
  request: QueuedSessionConfigRequest,
) {
  const remaining: SessionConfigWaiter[] = [];
  for (const waiter of queue.waiters) {
    if (waiter.sequence > request.sequence) {
      remaining.push(waiter);
      continue;
    }

    waiter.resolve({
      applied: sameSessionConfig(waiter.request, request),
    });
  }
  queue.waiters = remaining;
}

async function applyRequest(request: QueuedSessionConfigRequest) {
  await acpPrepareSession(
    request.sessionId,
    request.providerId,
    request.workingDir,
  );
  if (request.modelId) {
    await acpSetModel(request.sessionId, request.modelId);
  }
}

async function drainQueue(sessionId: string, queue: SessionConfigQueue) {
  if (queue.running) {
    return;
  }

  queue.running = true;
  try {
    while (queue.latest) {
      const request = queue.latest;
      try {
        await applyRequest(request);
      } catch (error) {
        if (queue.latest?.sequence !== request.sequence) {
          continue;
        }

        queue.latest = null;
        settleFailedWaiters(queue, request.sequence, error);
        break;
      }

      if (queue.latest?.sequence !== request.sequence) {
        continue;
      }

      queue.latest = null;
      settleAppliedWaiters(queue, request);
      break;
    }
  } finally {
    queue.running = false;
    if (queue.latest) {
      void drainQueue(sessionId, queue);
    } else if (queue.waiters.length === 0) {
      queues.delete(sessionId);
    }
  }
}

export function applyLatestSessionConfig(
  request: SessionConfigRequest,
): Promise<SessionConfigResult> {
  const queue = getQueue(request.sessionId);
  const sequence = queue.nextSequence + 1;
  queue.nextSequence = sequence;
  const queuedRequest = { ...request, sequence };
  queue.latest = queuedRequest;

  const result = new Promise<SessionConfigResult>((resolve, reject) => {
    queue.waiters.push({
      sequence,
      request: queuedRequest,
      resolve,
      reject,
    });
  });

  void drainQueue(request.sessionId, queue);
  return result;
}
