import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, type RenderOptions, screen, waitFor, fireEvent } from '@testing-library/react';
import { BottomMenuModeSelection } from './BottomMenuModeSelection';
import { IntlTestWrapper } from '../../i18n/test-utils';

const renderWithIntl = (ui: React.ReactElement, options?: RenderOptions) =>
  render(ui, { wrapper: IntlTestWrapper, ...options });

let mockConfig: Record<string, unknown> = {};
const mockUpdateSession = vi.fn().mockResolvedValue({});
const mockGetSession = vi.fn().mockResolvedValue({ data: null });

vi.mock('../ConfigContext', () => ({
  useConfig: () => ({
    config: mockConfig,
  }),
}));

vi.mock('../../utils/analytics', () => ({
  trackModeChanged: vi.fn(),
}));

vi.mock('../../api', () => ({
  updateSession: (...args: unknown[]) => mockUpdateSession(...args),
  getSession: (...args: unknown[]) => mockGetSession(...args),
}));

// Radix dropdown doesn't open in jsdom — render children directly
vi.mock('../ui/dropdown-menu', () => ({
  DropdownMenu: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  DropdownMenuTrigger: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  DropdownMenuContent: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  DropdownMenuItem: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
}));

describe('BottomMenuModeSelection', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockConfig = {};
  });

  it('displays mode from config when no session', async () => {
    mockConfig.GOOSE_MODE = 'approve';
    renderWithIntl(<BottomMenuModeSelection sessionId={null} />);
    await waitFor(() => {
      expect(screen.getByText('manual')).toBeInTheDocument();
    });
  });

  it('defaults to auto when config has no mode', async () => {
    mockConfig.GOOSE_MODE = undefined;
    renderWithIntl(<BottomMenuModeSelection sessionId={null} />);
    await waitFor(() => {
      expect(screen.getByText('autonomous')).toBeInTheDocument();
    });
  });

  it('fetches mode from session when sessionId is present', async () => {
    mockConfig.GOOSE_MODE = 'auto';
    mockGetSession.mockResolvedValue({ data: { goose_mode: 'approve' } });
    renderWithIntl(<BottomMenuModeSelection sessionId="test-session-123" />);
    await waitFor(() => {
      expect(screen.getByText('manual')).toBeInTheDocument();
    });
    expect(mockGetSession).toHaveBeenCalledWith({
      path: { session_id: 'test-session-123' },
    });
  });

  it('calls updateSession and does not write global config', async () => {
    mockConfig.GOOSE_MODE = 'auto';
    renderWithIntl(<BottomMenuModeSelection sessionId="test-session-123" />);

    fireEvent.click(screen.getByText('Manual'));

    await waitFor(() => {
      expect(mockUpdateSession).toHaveBeenCalledWith({
        body: { session_id: 'test-session-123', goose_mode: 'approve' },
      });
    });
  });

  it('does not call updateSession when sessionId is null', async () => {
    mockConfig.GOOSE_MODE = 'auto';
    renderWithIntl(<BottomMenuModeSelection sessionId={null} />);

    fireEvent.click(screen.getByText('Manual'));

    await waitFor(() => {
      expect(screen.getByText('manual')).toBeInTheDocument();
    });
    expect(mockUpdateSession).not.toHaveBeenCalled();
  });

  it('ignores stale session fetch after sessionId changes', async () => {
    let resolveA: (value: unknown) => void;
    const promiseA = new Promise((resolve) => {
      resolveA = resolve;
    });

    mockGetSession
      .mockImplementationOnce(() => promiseA)
      .mockResolvedValueOnce({ data: { goose_mode: 'auto' } });

    const { rerender } = renderWithIntl(<BottomMenuModeSelection sessionId="session-A" />);
    rerender(<BottomMenuModeSelection sessionId="session-B" />);

    await waitFor(() => {
      expect(screen.getByText('autonomous')).toBeInTheDocument();
    });

    resolveA!({ data: { goose_mode: 'approve' } });

    await waitFor(() => {
      expect(screen.getByText('autonomous')).toBeInTheDocument();
    });
    expect(screen.queryByText('manual')).not.toBeInTheDocument();
  });
});
