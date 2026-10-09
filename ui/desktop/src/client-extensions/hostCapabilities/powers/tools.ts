import { z } from 'zod';
import { callTool, listTools } from '../../../acp/permissions';
import { parsePayload } from '../payload';
import type { HostCapabilityDefinition } from '../types';

const listPayload = z.object({
  sessionId: z.string().trim().min(1),
  extensionName: z.string().trim().min(1).optional(),
});

const callPayload = z.object({
  sessionId: z.string().trim().min(1),
  extensionName: z.string().trim().min(1),
  name: z.string().trim().min(1),
  arguments: z.record(z.string(), z.unknown()).optional(),
});

export const toolsPower: HostCapabilityDefinition = {
  id: 'tools',
  description: 'List the tools in a session and call the ones set to Always allow.',
  methods: {
    list: {
      permission: 'tools:read',
      handle: async (_context, payload) => {
        const { sessionId, extensionName } = parsePayload(listPayload, payload);
        const tools = await listTools(sessionId, extensionName);
        return tools.map((tool) => ({
          name: tool.name,
          description: tool.description,
          permission: tool.permission ?? null,
          inputSchema: tool.inputSchema,
        }));
      },
    },
    call: {
      permission: 'tools:call',
      handle: async (_context, payload) => {
        const {
          sessionId,
          extensionName,
          name,
          arguments: args,
        } = parsePayload(callPayload, payload);
        const tools = await listTools(sessionId, extensionName);
        const tool = tools.find((candidate) => candidate.name === name);
        if (!tool) {
          throw new Error(`Unknown tool "${name}" in extension "${extensionName}"`);
        }
        if (tool.permission !== 'always_allow') {
          throw new Error(`Tool "${name}" must be set to Always allow before a plugin can call it`);
        }
        return callTool(sessionId, extensionName, name, args);
      },
    },
  },
};
