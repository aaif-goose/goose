export type ModelSwitchQueueInput = {
  becameAvailable: boolean;
  becameUnblocked: boolean;
  modelChangeSettled: boolean;
  isSubmissionBusy: boolean;
  queueProcessingBlocked: boolean;
  modelChangeInFlight: boolean;
  hasSendNowInFlight: boolean;
  hasQueuedMessages: boolean;
};

/** A model-settle edge must not submit, or drop, a queue that is still mid-turn. */
export function shouldProcessModelSwitchQueue(input: ModelSwitchQueueInput): boolean {
  const modelChangeSettledWhileIdle = input.modelChangeSettled && !input.isSubmissionBusy;
  return (
    (input.becameAvailable ||
      (input.becameUnblocked && !input.isSubmissionBusy) ||
      modelChangeSettledWhileIdle) &&
    !input.queueProcessingBlocked &&
    !input.modelChangeInFlight &&
    !input.hasSendNowInFlight &&
    input.hasQueuedMessages
  );
}
