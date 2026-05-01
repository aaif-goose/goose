import type { SessionExtensionStatusDto } from "@aaif/goose-sdk";
import { getClient } from "@/shared/api/acpConnection";
import type {
  ExtensionConfig,
  ExtensionEntry,
  SessionExtensionStatus,
} from "../types";

function toSessionExtensionStatus(
  extension: SessionExtensionStatusDto,
): SessionExtensionStatus {
  return {
    ...extension,
    tools: extension.tools ?? [],
    error: extension.error ?? undefined,
  };
}

export async function listExtensions(): Promise<ExtensionEntry[]> {
  const client = await getClient();
  const response = await client.goose.GooseConfigExtensions({});
  return response.extensions as ExtensionEntry[];
}

export async function listSessionExtensions(
  sessionId: string,
): Promise<SessionExtensionStatus[]> {
  const client = await getClient();
  const response = await client.goose.GooseSessionExtensionsStatus({
    sessionId,
  });
  return response.extensions.map(toSessionExtensionStatus);
}

export async function addExtension(
  name: string,
  extensionConfig: ExtensionConfig,
  enabled = false,
): Promise<void> {
  const client = await getClient();
  await client.goose.GooseConfigExtensionsAdd({
    name,
    extensionConfig,
    enabled,
  });
}

export async function removeExtension(configKey: string): Promise<void> {
  const client = await getClient();
  await client.goose.GooseConfigExtensionsRemove({ configKey });
}
