import { describe, expect, it } from 'vitest';
import { shouldProcessModelSwitchQueue, type ModelSwitchQueueInput } from './modelChangeQueue';

function input(overrides: Partial<ModelSwitchQueueInput> = {}): ModelSwitchQueueInput {
  return {
    becameAvailable: false,
    becameUnblocked: false,
    modelChangeSettled: false,
    isSubmissionBusy: false,
    queueProcessingBlocked: false,
    modelChangeInFlight: false,
    hasSendNowInFlight: false,
    hasQueuedMessages: true,
    ...overrides,
  };
}

describe('shouldProcessModelSwitchQueue', () => {
  it('does not drain a busy session when another switch settles', () => {
    expect(
      shouldProcessModelSwitchQueue(
        input({
          modelChangeSettled: true,
          isSubmissionBusy: true,
        })
      )
    ).toBe(false);
  });

  it('drains an idle session once its own switch settles', () => {
    expect(shouldProcessModelSwitchQueue(input({ modelChangeSettled: true }))).toBe(true);
  });

  it('still drains when the session becomes idle after a turn', () => {
    expect(shouldProcessModelSwitchQueue(input({ becameAvailable: true }))).toBe(true);
  });
});
