import type { SessionNotification } from '@agentclientprotocol/sdk';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { setAcpNotificationHandler } from '../acpConnection';
import {
  createAcpSessionNotificationRouter,
  installAcpSessionNotificationRouter,
} from '../sessionNotificationRouter';

vi.mock('../acpConnection', () => ({
  setAcpNotificationHandler: vi.fn(),
}));

function notification(sessionId: string): SessionNotification {
  return {
    sessionId,
    update: {
      sessionUpdate: 'session_info_update',
    },
  } as SessionNotification;
}

describe('sessionNotificationRouter', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('dispatches notifications only to subscribers for the matching session', async () => {
    const router = createAcpSessionNotificationRouter();
    const sessionOneListener = vi.fn();
    const sessionTwoListener = vi.fn();

    router.subscribe('session-1', sessionOneListener);
    router.subscribe('session-2', sessionTwoListener);

    await router.route(notification('session-1'));

    expect(sessionOneListener).toHaveBeenCalledTimes(1);
    expect(sessionOneListener).toHaveBeenCalledWith(notification('session-1'));
    expect(sessionTwoListener).not.toHaveBeenCalled();
  });

  it('supports multiple subscribers for one session', async () => {
    const router = createAcpSessionNotificationRouter();
    const firstListener = vi.fn();
    const secondListener = vi.fn();

    router.subscribe('session-1', firstListener);
    router.subscribe('session-1', secondListener);

    await router.route(notification('session-1'));

    expect(firstListener).toHaveBeenCalledTimes(1);
    expect(secondListener).toHaveBeenCalledTimes(1);
  });

  it('does not dispatch after unsubscribe', async () => {
    const router = createAcpSessionNotificationRouter();
    const listener = vi.fn();
    const unsubscribe = router.subscribe('session-1', listener);

    unsubscribe();
    await router.route(notification('session-1'));

    expect(listener).not.toHaveBeenCalled();
  });

  it('allows unsubscribe to be called more than once', async () => {
    const router = createAcpSessionNotificationRouter();
    const listener = vi.fn();
    const unsubscribe = router.subscribe('session-1', listener);

    unsubscribe();
    unsubscribe();
    await router.route(notification('session-1'));

    expect(listener).not.toHaveBeenCalled();
  });

  it('ignores notifications with no subscribers', async () => {
    const router = createAcpSessionNotificationRouter();

    await expect(router.route(notification('session-1'))).resolves.toBeUndefined();
  });

  it('installs the ACP handler explicitly and only once', async () => {
    installAcpSessionNotificationRouter();
    installAcpSessionNotificationRouter();

    expect(setAcpNotificationHandler).toHaveBeenCalledTimes(1);
    expect(setAcpNotificationHandler).toHaveBeenCalledWith({
      handleSessionNotification: expect.any(Function),
    });
  });
});
