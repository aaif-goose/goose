import type { RequestPermissionRequest, RequestPermissionResponse } from '@agentclientprotocol/sdk';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  cancelAcpPermissionRequestsForSession,
  requestAcpPermission,
  resolveAcpPermissionRequest,
} from '../permissionRequests';
import { acpChatSessionActions } from '../chatSessionStore';

vi.mock('../chatSessionStore', () => ({
  acpPermissionUserInputRequestId: (toolCallId: string) => `permission:${toolCallId}`,
  acpChatSessionActions: {
    applyPermissionRequest: vi.fn(),
    cancelPermissionRequest: vi.fn(),
    resolveUserInputRequest: vi.fn(),
  },
}));

function permissionRequest(sessionId: string, toolCallId: string): RequestPermissionRequest {
  return {
    sessionId,
    options: [
      { optionId: 'allow-once', name: 'Allow once', kind: 'allow_once' },
      { optionId: 'reject-once', name: 'Deny once', kind: 'reject_once' },
    ],
    toolCall: {
      toolCallId,
      title: 'Read file',
      rawInput: { path: 'README.md' },
      content: [
        {
          type: 'content',
          content: { type: 'text', text: 'Allow reading README.md?' },
        },
      ],
    },
  };
}

const TEST_SESSION_IDS = ['session-1', 'session-2'];

async function expectStillPending(promise: Promise<RequestPermissionResponse>): Promise<void> {
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

function appliedGeneration(callIndex = 0): string {
  return vi.mocked(acpChatSessionActions.applyPermissionRequest).mock.calls[callIndex][0]
    .generation;
}

describe('ACP permission requests', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    for (const sessionId of TEST_SESSION_IDS) {
      cancelAcpPermissionRequestsForSession(sessionId);
    }
  });

  afterEach(() => {
    for (const sessionId of TEST_SESSION_IDS) {
      cancelAcpPermissionRequestsForSession(sessionId);
    }
  });

  it('keeps permission requests pending until explicit resolve', async () => {
    const response = requestAcpPermission(permissionRequest('session-1', 'tool-1'));
    const generation = appliedGeneration();

    await expectStillPending(response);

    expect(resolveAcpPermissionRequest('session-1', 'tool-1', generation, 'allow_once')).toBe(true);
    expect(acpChatSessionActions.resolveUserInputRequest).toHaveBeenCalledWith(
      'session-1',
      'permission:tool-1'
    );
    await expect(response).resolves.toEqual({
      outcome: {
        outcome: 'selected',
        optionId: 'allow-once',
      },
    });
  });

  it('cancels only pending requests for the requested session', async () => {
    const sessionOneResponse = requestAcpPermission(permissionRequest('session-1', 'tool-1'));
    const sessionTwoResponse = requestAcpPermission(permissionRequest('session-2', 'tool-2'));
    const sessionTwoGeneration = appliedGeneration(1);

    cancelAcpPermissionRequestsForSession('session-1');

    await expect(sessionOneResponse).resolves.toEqual({
      outcome: {
        outcome: 'cancelled',
      },
    });
    await expectStillPending(sessionTwoResponse);
    expect(acpChatSessionActions.cancelPermissionRequest).toHaveBeenCalledWith(
      'session-1',
      'tool-1',
      appliedGeneration()
    );

    expect(
      resolveAcpPermissionRequest('session-2', 'tool-2', sessionTwoGeneration, 'deny_once')
    ).toBe(true);
    await expect(sessionTwoResponse).resolves.toEqual({
      outcome: {
        outcome: 'selected',
        optionId: 'reject-once',
      },
    });
  });

  it('cancels an older duplicate request for the same session and tool call', async () => {
    const firstResponse = requestAcpPermission(permissionRequest('session-1', 'tool-1'));
    const firstGeneration = appliedGeneration();
    const secondResponse = requestAcpPermission(permissionRequest('session-1', 'tool-1'));
    const secondGeneration = appliedGeneration(1);

    await expect(firstResponse).resolves.toEqual({
      outcome: {
        outcome: 'cancelled',
      },
    });
    await expectStillPending(secondResponse);
    expect(firstGeneration).not.toBe(secondGeneration);

    expect(resolveAcpPermissionRequest('session-1', 'tool-1', firstGeneration, 'allow_once')).toBe(
      false
    );
    await expectStillPending(secondResponse);

    expect(resolveAcpPermissionRequest('session-1', 'tool-1', secondGeneration, 'allow_once')).toBe(
      true
    );
    await expect(secondResponse).resolves.toEqual({
      outcome: {
        outcome: 'selected',
        optionId: 'allow-once',
      },
    });
  });

  it('fails closed when a legacy card has no permission generation', async () => {
    const response = requestAcpPermission(permissionRequest('session-1', 'tool-1'));

    expect(resolveAcpPermissionRequest('session-1', 'tool-1', undefined, 'allow_once')).toBe(false);
    await expectStillPending(response);
  });

  it('keeps distinct tool call IDs independently resolvable', async () => {
    const firstResponse = requestAcpPermission(permissionRequest('session-1', 'tool-1'));
    const firstGeneration = appliedGeneration();
    const secondResponse = requestAcpPermission(permissionRequest('session-1', 'tool-2'));
    const secondGeneration = appliedGeneration(1);

    expect(resolveAcpPermissionRequest('session-1', 'tool-1', firstGeneration, 'allow_once')).toBe(
      true
    );
    await expect(firstResponse).resolves.toMatchObject({ outcome: { outcome: 'selected' } });
    await expectStillPending(secondResponse);

    expect(resolveAcpPermissionRequest('session-1', 'tool-2', secondGeneration, 'deny_once')).toBe(
      true
    );
    await expect(secondResponse).resolves.toMatchObject({ outcome: { outcome: 'selected' } });
  });
});
