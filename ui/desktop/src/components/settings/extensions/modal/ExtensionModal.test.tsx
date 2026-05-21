import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, type RenderOptions, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import ExtensionModal from './ExtensionModal';
import { ExtensionFormData } from '../utils';
import { IntlTestWrapper } from '../../../../i18n/test-utils';
import { upsertConfig } from '../../../../api';

vi.mock('../../../../api', async () => {
  const actual = await vi.importActual<typeof import('../../../../api')>('../../../../api');
  return {
    ...actual,
    upsertConfig: vi.fn().mockResolvedValue({ data: 'ok' }),
  };
});

const mockedUpsertConfig = vi.mocked(upsertConfig);

const renderWithIntl = (ui: React.ReactElement, options?: RenderOptions) =>
  render(ui, { wrapper: IntlTestWrapper, ...options });

describe('ExtensionModal', () => {
  it('does not show unsaved changes dialog when closing without modifications', async () => {
    const user = userEvent.setup();
    const mockOnSubmit = vi.fn();
    const mockOnClose = vi.fn();

    const initialData: ExtensionFormData = {
      name: 'Existing Extension',
      description: 'An existing extension',
      type: 'stdio',
      cmd: 'npx some-mcp-server',
      endpoint: '',
      enabled: true,
      timeout: 300,
      envVars: [
        { key: 'API_KEY', value: '••••••••', isEdited: false },
        { key: 'OTHER_VAR', value: '••••••••', isEdited: false },
      ],
      headers: [],
    };

    renderWithIntl(
      <ExtensionModal
        title="Edit Extension"
        initialData={initialData}
        onClose={mockOnClose}
        onSubmit={mockOnSubmit}
        submitLabel="Save"
        modalType="edit"
      />
    );

    const cancelButton = screen.getByRole('button', { name: 'Cancel' });
    await user.click(cancelButton);

    expect(mockOnClose).toHaveBeenCalled();

    expect(screen.queryByText('Unsaved Changes')).not.toBeInTheDocument();
  });

  it('shows unsaved changes dialog when name is modified', async () => {
    const user = userEvent.setup();
    const mockOnSubmit = vi.fn();
    const mockOnClose = vi.fn();

    const initialData: ExtensionFormData = {
      name: 'Original Name',
      description: 'An existing extension',
      type: 'stdio',
      cmd: 'npx some-mcp-server',
      endpoint: '',
      enabled: true,
      timeout: 300,
      envVars: [],
      headers: [],
    };

    renderWithIntl(
      <ExtensionModal
        title="Edit Extension"
        initialData={initialData}
        onClose={mockOnClose}
        onSubmit={mockOnSubmit}
        submitLabel="Save"
        modalType="edit"
      />
    );

    const nameInput = screen.getByPlaceholderText('Enter extension name...');
    await user.clear(nameInput);
    await user.type(nameInput, 'New Name');

    const cancelButton = screen.getByRole('button', { name: 'Cancel' });
    await user.click(cancelButton);

    expect(screen.getByText('Unsaved Changes')).toBeInTheDocument();
    expect(mockOnClose).not.toHaveBeenCalled();
  });

  it('shows unsaved changes dialog when description is modified', async () => {
    const user = userEvent.setup();
    const mockOnSubmit = vi.fn();
    const mockOnClose = vi.fn();

    const initialData: ExtensionFormData = {
      name: 'Test Extension',
      description: 'Original description',
      type: 'stdio',
      cmd: 'npx some-mcp-server',
      endpoint: '',
      enabled: true,
      timeout: 300,
      envVars: [],
      headers: [],
    };

    renderWithIntl(
      <ExtensionModal
        title="Edit Extension"
        initialData={initialData}
        onClose={mockOnClose}
        onSubmit={mockOnSubmit}
        submitLabel="Save"
        modalType="edit"
      />
    );

    const descriptionInput = screen.getByPlaceholderText('Optional description...');
    await user.clear(descriptionInput);
    await user.type(descriptionInput, 'New description');

    const cancelButton = screen.getByRole('button', { name: 'Cancel' });
    await user.click(cancelButton);

    expect(screen.getByText('Unsaved Changes')).toBeInTheDocument();
    expect(mockOnClose).not.toHaveBeenCalled();
  });

  it('shows unsaved changes dialog when timeout is modified', async () => {
    const user = userEvent.setup();
    const mockOnSubmit = vi.fn();
    const mockOnClose = vi.fn();

    const initialData: ExtensionFormData = {
      name: 'Test Extension',
      description: 'An extension',
      type: 'stdio',
      cmd: 'npx some-mcp-server',
      endpoint: '',
      enabled: true,
      timeout: 300,
      envVars: [],
      headers: [],
    };

    renderWithIntl(
      <ExtensionModal
        title="Edit Extension"
        initialData={initialData}
        onClose={mockOnClose}
        onSubmit={mockOnSubmit}
        submitLabel="Save"
        modalType="edit"
      />
    );

    const timeoutInput = screen.getByDisplayValue('300');
    await user.clear(timeoutInput);
    await user.type(timeoutInput, '600');

    const cancelButton = screen.getByRole('button', { name: 'Cancel' });
    await user.click(cancelButton);

    expect(screen.getByText('Unsaved Changes')).toBeInTheDocument();
    expect(mockOnClose).not.toHaveBeenCalled();
  });

  it('creates a http_streamable extension', async () => {
    const user = userEvent.setup();
    const mockOnSubmit = vi.fn();
    const mockOnClose = vi.fn();

    const initialData: ExtensionFormData = {
      name: '',
      description: '',
      type: 'stdio', // Default type
      cmd: '',
      endpoint: '',
      enabled: true,
      timeout: 300,
      envVars: [],
      headers: [],
    };

    renderWithIntl(
      <ExtensionModal
        title="Add custom extension"
        initialData={initialData}
        onClose={mockOnClose}
        onSubmit={mockOnSubmit}
        submitLabel="Add Extension"
        modalType="add"
      />
    );

    const nameInput = screen.getByPlaceholderText('Enter extension name...');
    const submitButton = screen.getByTestId('extension-submit-btn');

    await user.type(nameInput, 'Test MCP');

    const typeSelect = screen.getByRole('combobox');
    await user.click(typeSelect);

    const httpOption = screen.getByText('Streamable HTTP');
    await user.click(httpOption);

    await waitFor(() => {
      expect(screen.getByText('Request Headers')).toBeInTheDocument();
    });

    const endpointInput = screen.getByPlaceholderText('Enter endpoint URL...');
    await user.type(endpointInput, 'https://foo.bar.com/mcp/');

    const descriptionInput = screen.getByPlaceholderText('Optional description...');
    await user.type(descriptionInput, 'Test MCP extension');

    const headerNameInput = screen.getByPlaceholderText('Header name');
    const headerValueInput = screen
      .getAllByPlaceholderText('Value')
      .find(
        (input) =>
          input.closest('div')?.textContent?.includes('Request Headers') ||
          input.parentElement?.parentElement?.textContent?.includes('Request Headers')
      );

    await user.type(headerNameInput, 'Authorization');
    if (headerValueInput) {
      await user.type(headerValueInput, 'Bearer abc123');
    }

    await user.click(submitButton);

    await waitFor(() => {
      expect(mockOnSubmit).toHaveBeenCalled();
    });

    const submittedData = mockOnSubmit.mock.calls[0][0];

    expect(submittedData.name).toBe('Test MCP');
    expect(submittedData.type).toBe('streamable_http');
    expect(submittedData.endpoint).toBe('https://foo.bar.com/mcp/');
    expect(submittedData.description).toBe('Test MCP extension');
    expect(submittedData.timeout).toBe(300);
    expect(submittedData.headers).toHaveLength(1);
    expect(submittedData.headers).toEqual([
      { key: 'Authorization', value: 'Bearer abc123', isEdited: true },
    ]);
  });

  describe('pending env var capture (fix for #8969)', () => {
    beforeEach(() => {
      mockedUpsertConfig.mockClear();
      mockedUpsertConfig.mockResolvedValue({
        data: 'ok',
        error: undefined,
        request: new globalThis.Request('http://localhost/test'),
        response: new globalThis.Response(),
      });
    });

    const emptyInitialData: ExtensionFormData = {
      name: '',
      description: '',
      type: 'stdio',
      cmd: '',
      endpoint: '',
      enabled: true,
      timeout: 300,
      envVars: [],
      headers: [],
    };

    // Returns the env-var key+value inputs (scoped to the "Environment Variables" section,
    // disambiguated from the header inputs which share the "Value" placeholder).
    function getEnvVarInputs() {
      const envVarKeyInput = screen.getByPlaceholderText('Variable name');
      const envVarValueInput = screen
        .getAllByPlaceholderText('Value')
        .find((input) =>
          input.parentElement?.parentElement?.parentElement?.textContent?.includes(
            'Environment Variables'
          )
        );
      return { envVarKeyInput, envVarValueInput };
    }

    it('captures a pending env var typed but not "+ Added" when Submit is clicked', async () => {
      const user = userEvent.setup();
      const mockOnSubmit = vi.fn();
      const mockOnClose = vi.fn();

      renderWithIntl(
        <ExtensionModal
          title="Add custom extension"
          initialData={emptyInitialData}
          onClose={mockOnClose}
          onSubmit={mockOnSubmit}
          submitLabel="Add Extension"
          modalType="add"
        />
      );

      await user.type(screen.getByPlaceholderText('Enter extension name...'), 'WooMCP');
      await user.type(
        screen.getByPlaceholderText(/^e\.g\. npx/),
        'npx -y @automattic/mcp-wordpress-remote@latest'
      );

      const { envVarKeyInput, envVarValueInput } = getEnvVarInputs();
      await user.type(envVarKeyInput, 'JWT_TOKEN');
      if (envVarValueInput) {
        await user.type(envVarValueInput, 'my_very_long_token');
      }

      // Note: intentionally NOT clicking the "+ Add" button — this is the #8969 repro.
      await user.click(screen.getByTestId('extension-submit-btn'));

      await waitFor(() => {
        expect(mockOnSubmit).toHaveBeenCalled();
      });

      expect(mockedUpsertConfig).toHaveBeenCalledWith(
        expect.objectContaining({
          body: expect.objectContaining({
            is_secret: true,
            key: 'JWT_TOKEN',
            value: 'my_very_long_token',
          }),
        })
      );

      const submittedData = mockOnSubmit.mock.calls[0][0];
      expect(submittedData.envVars).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            key: 'JWT_TOKEN',
            value: 'my_very_long_token',
            isEdited: true,
          }),
        ])
      );
    });

    it('does not capture a pending env var when only the key is filled', async () => {
      const user = userEvent.setup();
      const mockOnSubmit = vi.fn();
      const mockOnClose = vi.fn();

      renderWithIntl(
        <ExtensionModal
          title="Add custom extension"
          initialData={emptyInitialData}
          onClose={mockOnClose}
          onSubmit={mockOnSubmit}
          submitLabel="Add Extension"
          modalType="add"
        />
      );

      await user.type(screen.getByPlaceholderText('Enter extension name...'), 'WooMCP');
      await user.type(screen.getByPlaceholderText(/^e\.g\. npx/), 'npx -y something');

      const { envVarKeyInput } = getEnvVarInputs();
      await user.type(envVarKeyInput, 'LONELY_KEY');
      // Intentionally leaving the value field empty.

      await user.click(screen.getByTestId('extension-submit-btn'));

      await waitFor(() => {
        expect(mockOnSubmit).toHaveBeenCalled();
      });

      expect(mockedUpsertConfig).not.toHaveBeenCalledWith(
        expect.objectContaining({
          body: expect.objectContaining({ key: 'LONELY_KEY' }),
        })
      );

      const submittedData = mockOnSubmit.mock.calls[0][0];
      expect(submittedData.envVars).not.toEqual(
        expect.arrayContaining([expect.objectContaining({ key: 'LONELY_KEY' })])
      );
    });
  });
});
