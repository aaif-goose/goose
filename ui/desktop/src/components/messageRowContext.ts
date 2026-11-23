import {
  getAnyToolConfirmationData,
  getPendingToolConfirmationIds,
  getToolConfirmationContent,
  getToolRequests,
  getToolResponses,
  type Message,
  type ToolConfirmationData,
  type ToolResponseMessageContent,
} from '../types/message';
import { identifyConsecutiveToolCalls } from '../utils/toolCallChaining';

export interface ToolRenderState {
  requestId: string;
  response: ToolResponseMessageContent | undefined;
  confirmation: ToolConfirmationData | undefined;
  isPending: boolean;
}

export interface InferenceModel {
  id: string;
  requestedModel: string;
  /** False when the provider did not report which model served the message. */
  confirmed: boolean;
}

export interface MessageRowContext {
  hideTimestamp: boolean;
  isInToolCallChain: boolean;
  previousModel: InferenceModel | null;
  toolStates: readonly ToolRenderState[];
  toolConfirmationShownInline: boolean;
}

export function inferenceModel(message: Message): InferenceModel | null {
  if (message.role !== 'assistant' || !message.metadata.userVisible) return null;
  const inference = message.metadata.inference;
  if (!inference) return null;
  return {
    id: inference.resolvedModel || inference.requestedModel,
    requestedModel: inference.requestedModel,
    confirmed: Boolean(inference.resolvedModel),
  };
}

// A provider can report the resolved model for one reply and not for the next (for example,
// when its metadata lookup fails), so only two resolved models can be compared with each other.
export function isModelChange(previous: InferenceModel, current: InferenceModel): boolean {
  return previous.confirmed && current.confirmed
    ? previous.id !== current.id
    : previous.requestedModel !== current.requestedModel;
}

// A reply without a resolved model keeps the last resolved model of the same requested model,
// so a later resolved model is still compared with it.
function keepsPreviousModel(previous: InferenceModel | null, current: InferenceModel): boolean {
  return (
    previous !== null &&
    previous.confirmed &&
    !current.confirmed &&
    previous.requestedModel === current.requestedModel
  );
}

export function deriveMessageRowContexts(messages: Message[]): MessageRowContext[] {
  const toolCallChains = identifyConsecutiveToolCalls(messages);
  const chainedMessageIndices = new Set<number>();
  const hiddenTimestampIndices = new Set<number>();

  for (const chain of toolCallChains) {
    for (const messageIndex of chain) {
      chainedMessageIndices.add(messageIndex);
    }
    for (const messageIndex of chain.slice(0, -1)) {
      hiddenTimestampIndices.add(messageIndex);
    }
  }

  const toolRequestIds = new Set<string>();
  const firstConfirmationByRequestId = new Map<string, ToolConfirmationData>();

  for (const message of messages) {
    for (const request of getToolRequests(message)) {
      toolRequestIds.add(request.id);
    }

    const confirmation = getAnyToolConfirmationData(message);
    if (confirmation && !firstConfirmationByRequestId.has(confirmation.id)) {
      firstConfirmationByRequestId.set(confirmation.id, confirmation);
    }
  }

  const pendingConfirmationIds = getPendingToolConfirmationIds(messages);
  const toolStatesByMessageIndex: ToolRenderState[][] = Array.from(
    { length: messages.length },
    () => []
  );
  const latestResponseByRequestId = new Map<string, ToolResponseMessageContent>();
  const latestResponseMessageIndexByRequestId = new Map<string, number>();

  for (let messageIndex = messages.length - 1; messageIndex >= 0; messageIndex--) {
    const message = messages[messageIndex];
    toolStatesByMessageIndex[messageIndex] = getToolRequests(message).map((request) => ({
      requestId: request.id,
      response: latestResponseByRequestId.get(request.id),
      confirmation: firstConfirmationByRequestId.get(request.id),
      isPending: pendingConfirmationIds.has(request.id),
    }));

    for (const response of getToolResponses(message)) {
      const existingResponseMessageIndex = latestResponseMessageIndexByRequestId.get(response.id);
      if (
        existingResponseMessageIndex === undefined ||
        existingResponseMessageIndex === messageIndex
      ) {
        latestResponseByRequestId.set(response.id, response);
        latestResponseMessageIndexByRequestId.set(response.id, messageIndex);
      }
    }
  }

  let previousModel: InferenceModel | null = null;

  return messages.map((message, messageIndex) => {
    const currentModel = inferenceModel(message);
    const rowPreviousModel = currentModel ? previousModel : null;
    if (currentModel && !keepsPreviousModel(previousModel, currentModel)) {
      previousModel = currentModel;
    }

    const toolConfirmation = getToolConfirmationContent(message);
    const confirmationData = getAnyToolConfirmationData(message);

    return {
      hideTimestamp: hiddenTimestampIndices.has(messageIndex),
      isInToolCallChain: chainedMessageIndices.has(messageIndex),
      previousModel: rowPreviousModel,
      toolStates: toolStatesByMessageIndex[messageIndex],
      toolConfirmationShownInline: Boolean(
        toolConfirmation && confirmationData && toolRequestIds.has(confirmationData.id)
      ),
    };
  });
}
