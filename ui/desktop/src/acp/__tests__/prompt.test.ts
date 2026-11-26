import { methods } from '@agentclientprotocol/sdk/unstable';
import { describe, expect, it, vi } from 'vitest';
import type { Message } from '../../types/message';
import { acpRetryPrompt, messageToAcpPromptContent } from '../prompt';

const acp = vi.hoisted(() => ({
  getAcpClient: vi.fn(),
  getAcpInitializeResponse: vi.fn(),
}));
vi.mock('../acpConnection', () => acp);

describe('acpRetryPrompt', () => {
  it('retries by saved error ID through session/prompt', async () => {
    const request = vi.fn().mockResolvedValue({ stopReason: 'end_turn' });
    acp.getAcpClient.mockResolvedValueOnce({ connection: { agent: { request } } });
    acp.getAcpInitializeResponse.mockResolvedValueOnce({
      agentCapabilities: { _meta: { goose: { retryPrompt: {} } } },
    });

    await acpRetryPrompt('session-1', 'failure-1');

    expect(request).toHaveBeenCalledWith(methods.agent.session.prompt, {
      sessionId: 'session-1',
      prompt: [],
      _meta: { goose: { retryPromptErrorId: 'failure-1' } },
    });
  });

  it('does not send an empty prompt to a server without retry support', async () => {
    const request = vi.fn();
    acp.getAcpClient.mockResolvedValueOnce({ connection: { agent: { request } } });
    acp.getAcpInitializeResponse.mockResolvedValueOnce({ agentCapabilities: {} });

    await expect(acpRetryPrompt('session-1', 'failure-1')).rejects.toThrow(
      'does not support retrying failed turns'
    );
    expect(request).not.toHaveBeenCalled();
  });
});

describe('messageToAcpPromptContent', () => {
  it('converts text and image content into ACP prompt blocks', () => {
    const message: Message = {
      id: 'message-1',
      role: 'user',
      created: 123,
      content: [
        { type: 'text', text: 'Describe this' },
        {
          type: 'image',
          data: 'abc123',
          mimeType: 'image/png',
          _meta: { source: 'acp' },
          annotations: { priority: 0.5 },
        },
      ],
      metadata: { userVisible: true, agentVisible: true },
    };

    expect(messageToAcpPromptContent(message)).toEqual([
      { type: 'text', text: 'Describe this' },
      {
        type: 'image',
        data: 'abc123',
        mimeType: 'image/png',
        _meta: { source: 'acp' },
        annotations: { priority: 0.5 },
      },
    ]);
  });

  it('omits empty text content and unsupported content blocks', () => {
    const message: Message = {
      id: 'message-1',
      role: 'user',
      created: 123,
      content: [
        { type: 'text', text: '   ' },
        {
          type: 'toolResponse',
          id: 'tool-1',
          toolResult: { status: 'success', value: [] },
        },
      ],
      metadata: { userVisible: true, agentVisible: true },
    } as Message;

    expect(messageToAcpPromptContent(message)).toEqual([]);
  });
});
