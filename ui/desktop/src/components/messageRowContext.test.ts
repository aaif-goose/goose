import { describe, expect, it } from 'vitest';
import type { Message, MessageContent } from '../types/message';
import {
  deriveMessageRowContexts,
  type InferenceModel,
  inferenceModel,
  isModelChange,
} from './messageRowContext';

const visibleMetadata: Message['metadata'] = { agentVisible: true, userVisible: true };

function message(
  id: string,
  role: Message['role'],
  content: MessageContent[],
  resolvedModel?: string
): Message {
  return {
    id,
    role,
    created: 1,
    content,
    metadata: {
      ...visibleMetadata,
      ...(resolvedModel
        ? {
            inference: {
              provider: 'test',
              requestedModel: resolvedModel,
              resolvedModel,
            },
          }
        : {}),
    },
  };
}

function messageWithRequestedModel(id: string, requestedModel: string): Message {
  const base = message(id, 'assistant', [{ type: 'text', text: id }]);
  return {
    ...base,
    metadata: { ...base.metadata, inference: { provider: 'test', requestedModel } },
  };
}

function toolRequest(id: string): MessageContent {
  return {
    type: 'toolRequest',
    id,
    toolCall: {
      status: 'success',
      value: { name: 'test_tool', arguments: {} },
    },
  };
}

function toolResponse(id: string, value: string): MessageContent {
  return {
    type: 'toolResponse',
    id,
    toolResult: {
      status: 'success',
      value: { content: [{ type: 'text', text: value }], isError: false },
    },
  };
}

function toolConfirmation(id: string): MessageContent {
  return {
    type: 'actionRequired',
    data: {
      actionType: 'toolConfirmation',
      id,
      toolName: 'test_tool',
      arguments: {},
    },
  };
}

describe('deriveMessageRowContexts', () => {
  it('uses the last matching response after the request', () => {
    const messages = [
      message('response-before', 'user', [toolResponse('tool-1', 'before')]),
      message('request', 'assistant', [toolRequest('tool-1')]),
      message('response-after-1', 'user', [toolResponse('tool-1', 'after-1')]),
      message('response-after-2', 'user', [toolResponse('tool-1', 'after-2')]),
    ];

    const contexts = deriveMessageRowContexts(messages);

    expect(contexts[1].toolStates).toHaveLength(1);
    expect(contexts[1].toolStates[0].response).toEqual(toolResponse('tool-1', 'after-2'));
  });

  it('derives confirmation and pending state for each tool request', () => {
    const messages = [
      message('requests', 'assistant', [toolRequest('tool-1'), toolRequest('tool-2')]),
      message('confirmation-1', 'user', [toolConfirmation('tool-1')]),
      message('confirmation-2', 'user', [toolConfirmation('tool-2')]),
      message('response', 'user', [toolResponse('tool-1', 'complete')]),
    ];

    const contexts = deriveMessageRowContexts(messages);

    expect(contexts[0].toolStates).toMatchObject([
      {
        requestId: 'tool-1',
        confirmation: { id: 'tool-1', toolName: 'test_tool', arguments: {} },
        isPending: false,
      },
      {
        requestId: 'tool-2',
        confirmation: { id: 'tool-2', toolName: 'test_tool', arguments: {} },
        isPending: true,
      },
    ]);
    expect(contexts[1].toolConfirmationShownInline).toBe(true);
    expect(contexts[2].toolConfirmationShownInline).toBe(true);
  });

  it('preserves tool-call chain and timestamp behavior', () => {
    const messages = [
      message('tool-1', 'assistant', [
        { type: 'text', text: 'Starting tools.' },
        toolRequest('tool-1'),
      ]),
      message('tool-2', 'assistant', [toolRequest('tool-2')]),
      message('done', 'assistant', [{ type: 'text', text: 'Done.' }]),
    ];

    const contexts = deriveMessageRowContexts(messages);

    expect(contexts[0]).toMatchObject({ isInToolCallChain: true, hideTimestamp: true });
    expect(contexts[1]).toMatchObject({ isInToolCallChain: true, hideTimestamp: false });
    expect(contexts[2]).toMatchObject({ isInToolCallChain: false, hideTimestamp: false });
  });

  it('tracks the previous resolved model for model disclosures', () => {
    const messages = [
      message('model-a', 'assistant', [{ type: 'text', text: 'A' }], 'model-a'),
      message('user', 'user', [{ type: 'text', text: 'Continue' }]),
      message('model-b', 'assistant', [{ type: 'text', text: 'B' }], 'model-b'),
    ];

    const contexts = deriveMessageRowContexts(messages);

    expect(contexts[0].previousModel).toBeNull();
    expect(contexts[1].previousModel).toBeNull();
    expect(contexts[2].previousModel).toEqual({
      id: 'model-a',
      requestedModel: 'model-a',
      confirmed: true,
    });
  });

  it('falls back to the requested model when the provider does not report a resolved one', () => {
    const messages = [
      messageWithRequestedModel('model-a', 'model-a'),
      message('user', 'user', [{ type: 'text', text: 'Switch model' }]),
      messageWithRequestedModel('model-b', 'model-b'),
    ];

    const contexts = deriveMessageRowContexts(messages);

    expect(contexts[2].previousModel).toEqual({
      id: 'model-a',
      requestedModel: 'model-a',
      confirmed: false,
    });
    expect(inferenceModel(messages[2])).toEqual({
      id: 'model-b',
      requestedModel: 'model-b',
      confirmed: false,
    });
  });

  it('keeps the last resolved model across a reply without one', () => {
    const resolvedReply = (id: string, resolvedModel: string): Message => {
      const base = messageWithRequestedModel(id, 'model-a');
      return {
        ...base,
        metadata: {
          ...base.metadata,
          inference: { provider: 'test', requestedModel: 'model-a', resolvedModel },
        },
      };
    };
    const messages = [
      resolvedReply('first', 'model-a-1'),
      message('user-1', 'user', [{ type: 'text', text: 'Continue' }]),
      messageWithRequestedModel('second', 'model-a'),
      message('user-2', 'user', [{ type: 'text', text: 'Continue' }]),
      resolvedReply('third', 'model-a-2'),
    ];

    const contexts = deriveMessageRowContexts(messages);

    const firstModel = { id: 'model-a-1', requestedModel: 'model-a', confirmed: true };
    expect(contexts[2].previousModel).toEqual(firstModel);
    expect(contexts[4].previousModel).toEqual(firstModel);
  });
});

describe('isModelChange', () => {
  const resolved = (requestedModel: string, id: string): InferenceModel => ({
    id,
    requestedModel,
    confirmed: true,
  });
  const requested = (requestedModel: string): InferenceModel => ({
    id: requestedModel,
    requestedModel,
    confirmed: false,
  });

  it('compares resolved models when both replies report one', () => {
    expect(isModelChange(resolved('model-a', 'model-a-1'), resolved('model-a', 'model-a-2'))).toBe(
      true
    );
    expect(isModelChange(resolved('model-a', 'model-a-1'), resolved('model-b', 'model-a-1'))).toBe(
      false
    );
  });

  it('compares requested models when a reply has no resolved model', () => {
    expect(isModelChange(resolved('model-a', 'model-a-1'), requested('model-a'))).toBe(false);
    expect(isModelChange(requested('model-a'), resolved('model-a', 'model-a-1'))).toBe(false);
    expect(isModelChange(resolved('model-a', 'model-a-1'), requested('model-b'))).toBe(true);
    expect(isModelChange(requested('model-a'), requested('model-b'))).toBe(true);
  });
});
