import { describe, expect, it } from 'vitest';
import type { GooseExtension } from '@aaif/goose-acp-client';
import { extensionConfigToGooseExtension, gooseExtensionToExtensionConfig } from '../extensions';
import {
  createExtensionConfig,
  extensionToFormData,
} from '../../components/settings/extensions/utils';

const toolCallMeta = { a2ui: { clientCapabilities: { 'v0.9': { supportedCatalogIds: [] } } } };

const extensions: GooseExtension[] = [
  {
    type: 'mcp',
    server: { name: 'stdio', command: 'node', args: [], env: [], _meta: { toolCallMeta } },
  },
  {
    type: 'mcp',
    server: {
      type: 'http',
      name: 'http',
      url: 'https://example.com/mcp',
      headers: [],
      _meta: { toolCallMeta },
    },
  },
];

describe('ACP extension conversion', () => {
  it.each(extensions)('keeps toolCallMeta through a settings edit of $server.name', (extension) => {
    const config = gooseExtensionToExtensionConfig(extension)!;
    const edited = createExtensionConfig(extensionToFormData({ ...config, enabled: true }));
    const saved = extensionConfigToGooseExtension(edited)!;

    expect(saved.type === 'mcp' && saved.server._meta).toEqual({ toolCallMeta });
  });

  it('sends no _meta when there is no toolCallMeta', () => {
    const saved = extensionConfigToGooseExtension({
      type: 'stdio',
      name: 'plain',
      description: '',
      cmd: 'node',
    })!;

    expect(saved.type === 'mcp' && saved.server._meta).toBeUndefined();
  });
});
