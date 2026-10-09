export interface RegisteredPluginCommand {
  id: string;
  description: string;
  run: (args: unknown) => Promise<unknown> | unknown;
}

const commandsByExtension = new Map<string, Map<string, RegisteredPluginCommand>>();

export function pluginCommandId(extensionId: string, localId: string): string {
  return `${extensionId}:${localId}`;
}

export function registerPluginCommand(
  extensionId: string,
  localId: string,
  description: string,
  run: (args: unknown) => Promise<unknown> | unknown
): void {
  let commands = commandsByExtension.get(extensionId);
  if (!commands) {
    commands = new Map();
    commandsByExtension.set(extensionId, commands);
  }
  const id = pluginCommandId(extensionId, localId);
  commands.set(id, { id, description, run });
}

export function unregisterPluginCommands(extensionId: string): void {
  commandsByExtension.delete(extensionId);
}

export function listPluginCommands(): Array<{ id: string; description: string }> {
  const result: Array<{ id: string; description: string }> = [];
  for (const commands of commandsByExtension.values()) {
    for (const command of commands.values()) {
      result.push({ id: command.id, description: command.description });
    }
  }
  return result;
}

export function findPluginCommand(id: string): RegisteredPluginCommand | undefined {
  for (const commands of commandsByExtension.values()) {
    const found = commands.get(id);
    if (found) {
      return found;
    }
  }
  return undefined;
}
