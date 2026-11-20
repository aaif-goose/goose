import type {
  CreateElicitationRequest,
  CreateElicitationResponse,
} from '@agentclientprotocol/sdk/unstable';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  ACP_ELICITATION_TIMEOUT_SECONDS,
  cancelAcpElicitationRequestsForSession,
  requestAcpElicitation,
  resolveAcpElicitationRequest,
} from '../elicitationRequests';
import { acpChatSessionActions } from '../chatSessionStore';

vi.mock('../chatSessionStore', () => ({
  acpElicitationUserInputRequestId: (elicitationId: string) => `elicitation:${elicitationId}`,
  acpChatSessionActions: {
    applyElicitationRequest: vi.fn(),
    resolveUserInputRequest: vi.fn(),
    setElicitationStatus: vi.fn(),
  },
}));

const TEST_SESSION_IDS = ['session-1', 'session-2'];

function formRequest(sessionId: string): CreateElicitationRequest {
  return {
    mode: 'form',
    sessionId,
    message: 'Choose a project',
    requestedSchema: {
      type: 'object',
      properties: {
        project: {
          type: 'string',
        },
      },
      required: ['project'],
    },
  };
}

async function expectStillPending(promise: Promise<CreateElicitationResponse>): Promise<void> {
  let settled = false;
  promise.then(
    () => {
      settled = true;
    },
    () => {
      settled = true;
    }
  );

  await Promise.resolve();

  expect(settled).toBe(false);
}

describe('ACP elicitation requests', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    for (const sessionId of TEST_SESSION_IDS) {
      cancelAcpElicitationRequestsForSession(sessionId);
    }
  });

  afterEach(() => {
    for (const sessionId of TEST_SESSION_IDS) {
      cancelAcpElicitationRequestsForSession(sessionId);
    }
  });

  it('keeps form requests pending until explicit resolve', async () => {
    const response = requestAcpElicitation(formRequest('session-1'));

    await expectStillPending(response);

    const appliedRequest = vi.mocked(acpChatSessionActions.applyElicitationRequest).mock
      .calls[0][0];

    expect(appliedRequest.id).toMatch(/^acp_elicitation_/);
    expect(appliedRequest.sessionId).toBe('session-1');
    expect(appliedRequest.request.message).toBe('Choose a project');

    expect(
      resolveAcpElicitationRequest('session-1', appliedRequest.id, {
        project: 'goose',
      })
    ).toBe(true);
    expect(acpChatSessionActions.setElicitationStatus).toHaveBeenCalledWith(
      'session-1',
      appliedRequest.id,
      'submitted'
    );
    expect(acpChatSessionActions.resolveUserInputRequest).toHaveBeenCalledWith(
      'session-1',
      `elicitation:${appliedRequest.id}`
    );

    await expect(response).resolves.toEqual({
      action: 'accept',
      content: {
        project: 'goose',
      },
    });
  });

  it('cancels unsupported requests', async () => {
    await expect(
      requestAcpElicitation({
        mode: 'url',
        requestId: 'request-1',
        elicitationId: 'elicitation-1',
        message: 'Open this page',
        url: 'https://example.com',
      })
    ).resolves.toEqual({ action: 'cancel' });
  });

  it('cancels only pending requests for the requested session', async () => {
    const sessionOneResponse = requestAcpElicitation(formRequest('session-1'));
    const sessionTwoResponse = requestAcpElicitation(formRequest('session-2'));

    const applyElicitationRequest = vi.mocked(acpChatSessionActions.applyElicitationRequest);
    const sessionOneRequest = applyElicitationRequest.mock.calls[0][0];
    const sessionTwoRequest = applyElicitationRequest.mock.calls[1][0];

    cancelAcpElicitationRequestsForSession('session-1');

    expect(acpChatSessionActions.setElicitationStatus).toHaveBeenCalledWith(
      'session-1',
      sessionOneRequest.id,
      'cancelled'
    );
    await expect(sessionOneResponse).resolves.toEqual({ action: 'cancel' });
    await expectStillPending(sessionTwoResponse);

    expect(resolveAcpElicitationRequest('session-2', sessionTwoRequest.id, {})).toBe(true);
    await expect(sessionTwoResponse).resolves.toEqual({
      action: 'accept',
      content: {},
    });
    expect(resolveAcpElicitationRequest('session-1', sessionOneRequest.id, {})).toBe(false);
  });

  it('cancels pending requests when they expire', async () => {
    vi.useFakeTimers();
    try {
      const response = requestAcpElicitation(formRequest('session-1'));
      const appliedRequest = vi.mocked(acpChatSessionActions.applyElicitationRequest).mock
        .calls[0][0];

      await expectStillPending(response);

      await vi.advanceTimersByTimeAsync(ACP_ELICITATION_TIMEOUT_SECONDS * 1000);

      expect(acpChatSessionActions.setElicitationStatus).toHaveBeenCalledWith(
        'session-1',
        appliedRequest.id,
        'cancelled'
      );
      expect(acpChatSessionActions.resolveUserInputRequest).toHaveBeenCalledWith(
        'session-1',
        `elicitation:${appliedRequest.id}`
      );
      await expect(response).resolves.toEqual({ action: 'cancel' });
      expect(resolveAcpElicitationRequest('session-1', appliedRequest.id, {})).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });
});
