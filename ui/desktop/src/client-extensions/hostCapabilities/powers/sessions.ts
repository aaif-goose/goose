import { z } from 'zod';
import { acpListRecentSessions } from '../../../acp/sessions';
import { subscribeExtensionSessionEvents } from '../../extensionSessionEvents';
import { parsePayload } from '../payload';
import type { HostCapabilityDefinition } from '../types';

const DEFAULT_SESSION_LIMIT = 50;
const MAX_SESSION_LIMIT = 100;
const EVENTS_DISPOSER = 'events';

const listPayload = z.object({
  limit: z
    .number()
    .catch(DEFAULT_SESSION_LIMIT)
    .transform((limit) => Math.min(Math.max(Math.trunc(limit), 1), MAX_SESSION_LIMIT)),
});

export const sessionsPower: HostCapabilityDefinition = {
  id: 'sessions',
  description: 'List recent sessions and stream live session events.',
  methods: {
    list: {
      permission: 'sessions:read',
      handle: async (_context, payload) => {
        const { limit } = parsePayload(listPayload, payload);
        const sessions = await acpListRecentSessions(limit);
        return sessions.map((session) => ({
          id: session.id,
          name: session.name,
          workingDir: session.workingDir,
          createdAt: session.createdAt,
          updatedAt: session.updatedAt,
          lastMessageAt: session.lastMessageAt,
          messageCount: session.messageCount,
          providerId: session.providerId,
          modelId: session.modelId,
        }));
      },
    },
    subscribe: {
      permission: 'sessions:events',
      handle: (context) => {
        context.setDisposer(
          EVENTS_DISPOSER,
          subscribeExtensionSessionEvents((event) => context.emit('session', event))
        );
        return { subscribed: true };
      },
    },
    unsubscribe: {
      permission: 'sessions:events',
      handle: (context) => {
        context.clearDisposer(EVENTS_DISPOSER);
        return { subscribed: false };
      },
    },
  },
};
