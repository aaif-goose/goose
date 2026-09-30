import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../../../i18n/test-utils';
import type { ProviderDetails } from '../../../types/providers';
import { acpSetProviderEnabled } from '../../../acp/providers';
import ProviderGrid from './ProviderGrid';

vi.mock('../../../acp/providers', () => ({ acpSetProviderEnabled: vi.fn() }));
vi.mock('../../ModelAndProviderContext', () => ({
  useModelAndProvider: () => ({ getCurrentModelAndProvider: vi.fn() }),
}));
vi.mock('./modal/ProviderConfigurationModal', () => ({ default: () => null }));
vi.mock('./modal/subcomponents/forms/CustomProviderForm', () => ({ default: () => null }));
vi.mock('../models/subcomponents/SwitchModelModal', () => ({ SwitchModelModal: () => null }));

function provider(name: string, deprecated: boolean, enabled: boolean): ProviderDetails {
  return {
    name,
    deprecated,
    is_enabled: enabled,
    is_configured: true,
    is_available: true,
    visible_in_setup: true,
    provider_type: 'Builtin',
    uses_acp: false,
    metadata: {
      name,
      display_name: name,
      description: 'Provider description',
      default_model: 'model',
      known_models: [],
      model_doc_link: '',
      config_keys: [],
    },
  };
}
const providers = [
  provider('current', false, false),
  provider('legacy-enabled', true, true),
  provider('legacy-disabled', true, false),
];
function renderGrid(items = providers) {
  return render(<ProviderGrid providers={items} isOnboarding={false} />, {
    wrapper: IntlTestWrapper,
  });
}
describe('ProviderGrid deprecated providers', () => {
  it('hides disabled deprecated providers by default, even with saved credentials', () => {
    renderGrid();
    expect(screen.getByTestId('provider-card-current')).toBeInTheDocument();
    expect(screen.getByTestId('provider-card-legacy-enabled')).toBeInTheDocument();
    expect(screen.queryByTestId('provider-card-legacy-disabled')).not.toBeInTheDocument();
  });
  it('reveals all or only deprecated providers and combines the filter with search', async () => {
    renderGrid();
    const filter = screen.getByRole('combobox', { name: 'Filter providers' });
    await userEvent.selectOptions(filter, 'all');
    expect(screen.getByTestId('provider-card-current')).toBeInTheDocument();
    expect(screen.getByTestId('provider-card-legacy-disabled')).toBeInTheDocument();
    await userEvent.selectOptions(filter, 'deprecated');
    expect(screen.queryByTestId('provider-card-current')).not.toBeInTheDocument();
    expect(screen.getByTestId('provider-card-legacy-enabled')).toBeInTheDocument();
    expect(screen.getByTestId('provider-card-legacy-disabled')).toBeInTheDocument();
    await userEvent.type(screen.getByRole('searchbox'), 'disabled');
    expect(screen.queryByTestId('provider-card-legacy-enabled')).not.toBeInTheDocument();
    expect(screen.getByTestId('provider-card-legacy-disabled')).toBeInTheDocument();
    await userEvent.selectOptions(filter, 'default');
    expect(screen.queryByTestId('provider-card-legacy-disabled')).not.toBeInTheDocument();
    expect(screen.getByText('No providers match "disabled"')).toBeInTheDocument();
  });
  it('removes a deprecated provider after disabling and refreshing', async () => {
    vi.mocked(acpSetProviderEnabled).mockResolvedValue();
    const refresh = vi.fn();
    const { rerender } = render(
      <ProviderGrid providers={providers} isOnboarding={false} refreshProviders={refresh} />,
      { wrapper: IntlTestWrapper }
    );
    await userEvent.click(screen.getByRole('button', { name: 'Disable legacy-enabled' }));
    await waitFor(() => expect(refresh).toHaveBeenCalledOnce());
    expect(acpSetProviderEnabled).toHaveBeenCalledWith('legacy-enabled', false);
    rerender(
      <ProviderGrid
        providers={providers.map((item) => ({ ...item, is_enabled: false }))}
        isOnboarding={false}
        refreshProviders={refresh}
      />
    );
    expect(screen.queryByTestId('provider-card-legacy-enabled')).not.toBeInTheDocument();
  });
  it('explains an empty deprecated filter', async () => {
    renderGrid([providers[0]]);
    await userEvent.selectOptions(screen.getByRole('combobox'), 'deprecated');
    expect(screen.getByText('No deprecated providers')).toBeInTheDocument();
    expect(screen.getByTestId('add-custom-provider-card')).toBeInTheDocument();
  });
});
