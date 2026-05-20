import type { ExtensionResponse, ExtensionEntry } from '../api';
import { getAcpClient } from './acpConnection';

export async function getConfiguredExtensions(): Promise<ExtensionResponse> {
  const client = await getAcpClient();
  const response = await client.goose.configExtensionsList_unstable({});
  return {
    extensions: response.extensions as ExtensionEntry[],
    warnings: response.warnings,
  };
}
