import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { acpSetProviderEnabled } from '../../../../acp/providers';
import { IntlTestWrapper } from '../../../../i18n/test-utils';
import type { ProviderDetails } from '../../../../types/providers';
import { ProviderCard } from './ProviderCard';

vi.mock('../../../../acp/providers', () => ({ acpSetProviderEnabled: vi.fn() }));

const provider: ProviderDetails = {
  name: 'aws_bedrock',
  is_enabled: true,
  is_configured: true,
  is_available: true,
  visible_in_setup: true,
  deprecated: false,
  provider_type: 'Builtin',
  uses_acp: false,
  metadata: {
    name: 'aws_bedrock',
    display_name: 'Amazon Bedrock',
    description: '',
    default_model: 'saved-model',
    known_models: [],
    model_doc_link: '',
    config_keys: [],
  },
};

describe('ProviderCard enablement', () => {
  beforeEach(() => vi.clearAllMocks());

  it('disables a configured provider without opening its configuration form', async () => {
    vi.mocked(acpSetProviderEnabled).mockResolvedValue();
    const refresh = vi.fn();
    const configure = vi.fn();
    render(
      <ProviderCard
        provider={provider}
        onConfigure={configure}
        onLaunch={vi.fn()}
        isOnboarding={false}
        onEnablementChanged={refresh}
      />,
      { wrapper: IntlTestWrapper }
    );
    await userEvent.click(screen.getByRole('button', { name: 'Disable Amazon Bedrock' }));
    await waitFor(() => expect(refresh).toHaveBeenCalledOnce());
    expect(acpSetProviderEnabled).toHaveBeenCalledWith('aws_bedrock', false);
    expect(configure).not.toHaveBeenCalled();
  });

  it('offers re-enable even when credentials are still detected', async () => {
    vi.mocked(acpSetProviderEnabled).mockResolvedValue();
    render(
      <ProviderCard
        provider={{ ...provider, is_enabled: false }}
        onConfigure={vi.fn()}
        onLaunch={vi.fn()}
        isOnboarding={false}
      />,
      { wrapper: IntlTestWrapper }
    );
    await userEvent.click(screen.getByRole('button', { name: 'Enable Amazon Bedrock' }));
    expect(acpSetProviderEnabled).toHaveBeenCalledWith('aws_bedrock', true);
  });

  it('reports a failed save and retains the existing choice', async () => {
    vi.mocked(acpSetProviderEnabled).mockRejectedValue(new Error('Cannot save settings'));
    const refresh = vi.fn();
    render(
      <ProviderCard
        provider={provider}
        onConfigure={vi.fn()}
        onLaunch={vi.fn()}
        isOnboarding={false}
        onEnablementChanged={refresh}
      />,
      { wrapper: IntlTestWrapper }
    );
    await userEvent.click(screen.getByRole('button', { name: 'Disable Amazon Bedrock' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Cannot save settings');
    expect(refresh).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: 'Disable Amazon Bedrock' })).toBeEnabled();
  });
});
