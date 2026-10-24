import { z } from 'zod';
import { findPluginCommand, listPluginCommands } from '../../pluginCommandRegistry';
import { parsePayload } from '../payload';
import type { HostActions, HostCapabilityDefinition } from '../types';

interface CoreCommand {
  description: string;
  run: (actions: HostActions, args: unknown) => Promise<unknown> | unknown;
}

const MAX_PROMPT_LENGTH = 20_000;

const executePayload = z.object({
  command: z.string().trim().min(1),
  args: z.unknown().optional(),
});

const newChatArgs = z.object({
  prompt: z.string().min(1).max(MAX_PROMPT_LENGTH).optional(),
  recipeId: z.string().trim().min(1).optional(),
  workingDir: z.string().trim().min(1).optional(),
});

const openSessionArgs = z.object({ sessionId: z.string().trim().min(1) });

const openPageArgs = z.object({
  extensionId: z.string().trim().min(1),
  viewId: z.string().trim().min(1),
});

const CORE_COMMANDS = new Map<string, CoreCommand>([
  [
    'chat.new',
    {
      description: 'Start a chat, optionally with a first prompt or a recipe.',
      run: async (actions, args) => ({
        sessionId: await actions.startChat(parsePayload(newChatArgs, args)),
      }),
    },
  ],
  [
    'session.open',
    {
      description: 'Open an existing session.',
      run: (actions, args) => {
        const { sessionId } = parsePayload(openSessionArgs, args);
        actions.openSession(sessionId);
        return { sessionId };
      },
    },
  ],
  [
    'plugin.open',
    {
      description: 'Open a plugin page.',
      run: (actions, args) => {
        const { extensionId, viewId } = parsePayload(openPageArgs, args);
        actions.openPage(extensionId, viewId);
        return { extensionId, viewId };
      },
    },
  ],
]);

export const commandsPower: HostCapabilityDefinition = {
  id: 'commands',
  description: 'List and run Goose commands.',
  methods: {
    list: {
      permission: 'commands:execute',
      handle: () => [
        ...[...CORE_COMMANDS].map(([id, command]) => ({ id, description: command.description })),
        ...listPluginCommands(),
      ],
    },
    execute: {
      permission: 'commands:execute',
      handle: (context, payload) => {
        const { command, args } = parsePayload(executePayload, payload);
        const coreCommand = CORE_COMMANDS.get(command);
        if (coreCommand) {
          return coreCommand.run(context.actions, args);
        }
        const pluginCommand = findPluginCommand(command);
        if (!pluginCommand) {
          throw new Error(`Unknown command "${command}"`);
        }
        return pluginCommand.run(args);
      },
    },
  },
};
