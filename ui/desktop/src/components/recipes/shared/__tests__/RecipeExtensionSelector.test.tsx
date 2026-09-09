import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, type RenderOptions, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { RecipeExtensionSelector } from '../RecipeExtensionSelector';
import { IntlTestWrapper } from '../../../../i18n/test-utils';
import type { FixedExtensionEntry } from '../../../ConfigContext';

const configContextMock = vi.hoisted(() => ({
  extensionsList: [] as FixedExtensionEntry[],
}));

vi.mock('../../../ConfigContext', () => ({
  useConfig: () => ({
    extensionsList: configContextMock.extensionsList,
  }),
}));

const renderWithIntl = (ui: React.ReactElement, options?: RenderOptions) =>
  render(ui, { wrapper: IntlTestWrapper, ...options });

describe('RecipeExtensionSelector', () => {
  beforeEach(() => {
    configContextMock.extensionsList = [];
  });

  it('preserves non-empty available tools when selecting a configured extension', async () => {
    const user = userEvent.setup();
    const onExtensionsChange = vi.fn();
    configContextMock.extensionsList = [
      {
        type: 'builtin',
        name: 'developer',
        description: 'Developer tools',
        enabled: true,
        available_tools: ['shell', 'read_file'],
      },
    ];

    renderWithIntl(
      <RecipeExtensionSelector selectedExtensions={[]} onExtensionsChange={onExtensionsChange} />
    );

    await user.click(screen.getByText('Developer'));

    expect(onExtensionsChange).toHaveBeenCalledWith([
      expect.objectContaining({
        type: 'builtin',
        name: 'developer',
        available_tools: ['shell', 'read_file'],
      }),
    ]);
  });

  it('preserves static OAuth fields when selecting a configured extension', async () => {
    const user = userEvent.setup();
    const onExtensionsChange = vi.fn();
    configContextMock.extensionsList = [
      {
        type: 'streamable_http',
        name: 'google_workspace',
        description: 'Google Workspace',
        uri: 'https://example.com/mcp',
        client_id: 'registered-client',
        client_secret_key: 'GOOGLE_OAUTH_SECRET',
        scopes: ['drive.readonly'],
        enabled: true,
      },
    ];

    renderWithIntl(
      <RecipeExtensionSelector selectedExtensions={[]} onExtensionsChange={onExtensionsChange} />
    );

    await user.click(screen.getByTitle('Google Workspace'));

    expect(onExtensionsChange).toHaveBeenCalledWith([
      expect.objectContaining({
        type: 'streamable_http',
        name: 'google_workspace',
        client_id: 'registered-client',
        client_secret_key: 'GOOGLE_OAUTH_SECRET',
        scopes: ['drive.readonly'],
      }),
    ]);
  });
});
