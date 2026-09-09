import { render, type RenderOptions, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { resolveAcpPermissionRequest } from '../acp/permissionRequests';
import { IntlTestWrapper } from '../i18n/test-utils';
import ToolApprovalButtons from './ToolApprovalButtons';

vi.mock('../acp/permissionRequests', () => ({
  resolveAcpPermissionRequest: vi.fn(),
}));

const renderWithIntl = (ui: React.ReactElement, options?: RenderOptions) =>
  render(ui, { wrapper: IntlTestWrapper, ...options });

const resolveAcpPermissionRequestMock = vi.mocked(resolveAcpPermissionRequest);

describe('ToolApprovalButtons', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('marks the approval accepted when the ACP request resolves', async () => {
    resolveAcpPermissionRequestMock.mockReturnValueOnce(true);

    renderWithIntl(
      <ToolApprovalButtons
        data={{
          id: 'tool-call-approved',
          generation: 'permission-generation-1',
          toolName: 'developer__shell',
          sessionId: 'session-1',
        }}
      />
    );

    await userEvent.click(screen.getByRole('button', { name: 'Allow Once' }));

    expect(resolveAcpPermissionRequestMock).toHaveBeenCalledWith(
      'session-1',
      'tool-call-approved',
      'permission-generation-1',
      'allow_once'
    );
    expect(screen.getByText('developer__shell - Allowed once')).toBeInTheDocument();
  });

  it('shows a stale request error when ACP has no pending request', async () => {
    resolveAcpPermissionRequestMock.mockReturnValueOnce(false);

    renderWithIntl(
      <ToolApprovalButtons
        data={{
          id: 'tool-call-rerun',
          toolName: 'developer__shell',
          sessionId: 'session-1',
        }}
      />
    );

    await userEvent.click(screen.getByRole('button', { name: 'Allow Once' }));

    expect(resolveAcpPermissionRequestMock).toHaveBeenCalledWith(
      'session-1',
      'tool-call-rerun',
      undefined,
      'allow_once'
    );
    expect(screen.getByText('This approval request is no longer active.')).toBeInTheDocument();
    expect(screen.queryByText('developer__shell - Allowed once')).not.toBeInTheDocument();
  });

  it('resets the displayed decision for a new permission generation', async () => {
    resolveAcpPermissionRequestMock.mockReturnValue(true);
    const { rerender } = renderWithIntl(
      <ToolApprovalButtons
        data={{
          id: 'tool-call-reused',
          generation: 'permission-generation-a',
          toolName: 'developer__shell',
          sessionId: 'session-1',
        }}
      />
    );

    await userEvent.click(screen.getByRole('button', { name: 'Allow Once' }));
    expect(screen.getByText('developer__shell - Allowed once')).toBeInTheDocument();

    rerender(
      <ToolApprovalButtons
        data={{
          id: 'tool-call-reused',
          generation: 'permission-generation-b',
          toolName: 'developer__shell',
          sessionId: 'session-1',
        }}
      />
    );
    await userEvent.click(await screen.findByRole('button', { name: 'Allow Once' }));

    expect(resolveAcpPermissionRequestMock).toHaveBeenLastCalledWith(
      'session-1',
      'tool-call-reused',
      'permission-generation-b',
      'allow_once'
    );
  });
});
