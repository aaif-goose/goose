import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, type RenderOptions, screen, fireEvent, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { AlertBox } from '../AlertBox';
import { Alert, AlertType } from '../types';
import { IntlTestWrapper } from '../../../i18n/test-utils';

const renderWithIntl = (ui: React.ReactElement, options?: RenderOptions) =>
  render(ui, { wrapper: IntlTestWrapper, ...options });

const { mockRead, mockUpsert } = vi.hoisted(() => ({
  mockRead: vi.fn(),
  mockUpsert: vi.fn(),
}));

vi.mock('../../ConfigContext', () => ({
  useConfig: () => ({ read: mockRead, upsert: mockUpsert }),
}));

describe('AlertBox', () => {
  const mockOnCompact = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    mockRead.mockImplementation(async (key: string) =>
      key === 'GOOSE_AUTO_COMPACT_THRESHOLD' ? 0.8 : 225000
    );
    mockUpsert.mockResolvedValue(undefined);
  });

  describe('Basic Rendering', () => {
    it('should render info alert with message', () => {
      const alert: Alert = {
        type: AlertType.Info,
        message: 'Test info message',
      };

      renderWithIntl(<AlertBox alert={alert} />);

      expect(screen.getByText('Test info message')).toBeInTheDocument();
    });

    it('should render warning alert with correct styling', () => {
      const alert: Alert = {
        type: AlertType.Warning,
        message: 'Test warning message',
      };

      const { container } = renderWithIntl(<AlertBox alert={alert} />);
      const alertElement = container.querySelector('.bg-\\[\\#cc4b03\\]');

      expect(alertElement).toBeInTheDocument();
      expect(screen.getByText('Test warning message')).toBeInTheDocument();
    });

    it('should render error alert with correct styling', () => {
      const alert: Alert = {
        type: AlertType.Error,
        message: 'Test error message',
      };

      const { container } = renderWithIntl(<AlertBox alert={alert} />);
      const alertElement = container.querySelector('.bg-\\[\\#d7040e\\]');

      expect(alertElement).toBeInTheDocument();
      expect(screen.getByText('Test error message')).toBeInTheDocument();
    });

    it('should apply custom className', () => {
      const alert: Alert = {
        type: AlertType.Info,
        message: 'Test message',
      };

      const { container } = renderWithIntl(<AlertBox alert={alert} className="custom-class" />);
      const alertElement = container.firstChild as HTMLElement;

      expect(alertElement).toHaveClass('custom-class');
    });
  });

  describe('Progress Alert', () => {
    it('should render auto-compact threshold when progress is provided', async () => {
      const alert: Alert = {
        type: AlertType.Info,
        message: 'Context window',
        progress: {
          current: 50,
          total: 100,
        },
      };

      renderWithIntl(<AlertBox alert={alert} />);

      // Should show auto-compact threshold (default 80%)
      expect(await screen.findByText(/Auto compact at 80%/)).toBeInTheDocument();
    });

    it('should not render progress dots or token counts', () => {
      const alert: Alert = {
        type: AlertType.Info,
        message: 'Context window',
        progress: {
          current: 1500,
          total: 10000,
        },
      };

      const { container } = renderWithIntl(<AlertBox alert={alert} />);

      // Progress dots and token counts are no longer rendered
      expect(screen.queryByText('1.5k')).not.toBeInTheDocument();
      expect(screen.queryByText('10k')).not.toBeInTheDocument();
      expect(screen.queryByText('15%')).not.toBeInTheDocument();
      const progressDots = container.querySelectorAll('.h-\\[2px\\]');
      expect(progressDots.length).toBe(0);
    });
  });

  describe('Effective compaction threshold', () => {
    const progressAlert = (total: number): Alert => ({
      type: AlertType.Info,
      message: 'Context window',
      progress: { current: 100000, total },
    });

    it('edits the displayed token cap on a 1M-token model', async () => {
      renderWithIntl(<AlertBox alert={progressAlert(1000000)} />);
      expect(await screen.findByText('Auto compact at 225k')).toBeInTheDocument();
      fireEvent.click(screen.getByRole('button'));
      const input = screen.getByRole('spinbutton');
      expect(input).toHaveValue(225000);
      fireEvent.change(input, { target: { value: '250000' } });
      fireEvent.keyDown(input, { key: 'Enter' });
      expect(await screen.findByText('Auto compact at 250k')).toBeInTheDocument();
      expect(mockUpsert).toHaveBeenCalledExactlyOnceWith(
        'GOOSE_AUTO_COMPACT_TOKEN_LIMIT',
        250000,
        false
      );
    });

    it('edits the percentage in an uncapped context', async () => {
      const onThresholdChange = vi.fn();
      renderWithIntl(<AlertBox alert={{ ...progressAlert(200000), onThresholdChange }} />);
      expect(await screen.findByText('Auto compact at 80%')).toBeInTheDocument();
      fireEvent.click(screen.getByRole('button'));
      const input = screen.getByRole('spinbutton');
      expect(input).toHaveValue(80);
      fireEvent.change(input, { target: { value: '70' } });
      fireEvent.keyDown(input, { key: 'Enter' });
      expect(await screen.findByText('Auto compact at 70%')).toBeInTheDocument();
      expect(mockUpsert).toHaveBeenCalledExactlyOnceWith(
        'GOOSE_AUTO_COMPACT_THRESHOLD',
        0.7,
        false
      );
      expect(onThresholdChange).toHaveBeenCalledWith(0.7);
    });

    it('shows the percentage when an edited cap exceeds the percentage budget', async () => {
      renderWithIntl(<AlertBox alert={progressAlert(1000000)} />);
      await screen.findByText('Auto compact at 225k');
      fireEvent.click(screen.getByRole('button'));
      const input = screen.getByRole('spinbutton');
      fireEvent.change(input, { target: { value: '900000' } });
      fireEvent.keyDown(input, { key: 'Enter' });
      await waitFor(() =>
        expect(mockUpsert).toHaveBeenCalledWith('GOOSE_AUTO_COMPACT_TOKEN_LIMIT', 900000, false)
      );
      expect(screen.getByText('Auto compact at 80%')).toBeInTheDocument();
    });

    it('recalculates the displayed setting when the model context changes', async () => {
      const { rerender } = renderWithIntl(<AlertBox alert={progressAlert(1000000)} />);
      await screen.findByText('Auto compact at 225k');
      rerender(<AlertBox alert={progressAlert(200000)} />);
      expect(screen.getByText('Auto compact at 80%')).toBeInTheDocument();
      rerender(<AlertBox alert={progressAlert(1000000)} />);
      expect(screen.getByText('Auto compact at 225k')).toBeInTheDocument();
      expect(mockUpsert).not.toHaveBeenCalled();
    });

    it('keeps the edited setting stable if the model changes during editing', async () => {
      const { rerender } = renderWithIntl(<AlertBox alert={progressAlert(1000000)} />);
      await screen.findByText('Auto compact at 225k');
      fireEvent.click(screen.getByRole('button'));
      const input = screen.getByRole('spinbutton');
      fireEvent.change(input, { target: { value: '250000' } });
      rerender(<AlertBox alert={progressAlert(200000)} />);
      expect(input).toHaveValue(250000);
      fireEvent.keyDown(input, { key: 'Enter' });
      await waitFor(() =>
        expect(mockUpsert).toHaveBeenCalledExactlyOnceWith(
          'GOOSE_AUTO_COMPACT_TOKEN_LIMIT',
          250000,
          false
        )
      );
      expect(screen.getByText('Auto compact at 80%')).toBeInTheDocument();
    });

    it('discards a cancelled cap edit', async () => {
      renderWithIntl(<AlertBox alert={progressAlert(1000000)} />);
      await screen.findByText('Auto compact at 225k');
      fireEvent.click(screen.getByRole('button'));
      const input = screen.getByRole('spinbutton');
      fireEvent.change(input, { target: { value: '250000' } });
      fireEvent.keyDown(input, { key: 'Escape' });
      expect(screen.getByText('Auto compact at 225k')).toBeInTheDocument();
      fireEvent.click(screen.getByRole('button'));
      expect(screen.getByRole('spinbutton')).toHaveValue(225000);
      expect(mockUpsert).not.toHaveBeenCalled();
    });

    it('keeps disabled compaction disabled across context changes', async () => {
      mockRead.mockImplementation(async (key: string) =>
        key === 'GOOSE_AUTO_COMPACT_THRESHOLD' ? 0 : 225000
      );
      const { rerender } = renderWithIntl(<AlertBox alert={progressAlert(1000000)} />);
      await screen.findByText('Auto compact at 0%');
      rerender(<AlertBox alert={progressAlert(200000)} />);
      expect(screen.getByText('Auto compact at 0%')).toBeInTheDocument();
      expect(mockUpsert).not.toHaveBeenCalled();
    });
  });

  describe('Compact Button', () => {
    it('should render compact button when showCompactButton is true', () => {
      const alert: Alert = {
        type: AlertType.Info,
        message: 'Context window',
        progress: { current: 50, total: 100 },
        showCompactButton: true,
        onCompact: mockOnCompact,
      };

      renderWithIntl(<AlertBox alert={alert} />);

      expect(screen.getByText('Compact now')).toBeInTheDocument();
    });

    it('should render compact button with custom icon', () => {
      const CompactIcon = () => <span data-testid="compact-icon">📦</span>;

      const alert: Alert = {
        type: AlertType.Info,
        message: 'Context window',
        progress: { current: 50, total: 100 },
        showCompactButton: true,
        onCompact: mockOnCompact,
        compactIcon: <CompactIcon />,
      };

      renderWithIntl(<AlertBox alert={alert} />);

      expect(screen.getByTestId('compact-icon')).toBeInTheDocument();
      expect(screen.getByText('Compact now')).toBeInTheDocument();
    });

    it('should call onCompact when compact button is clicked', async () => {
      const user = userEvent.setup();

      const alert: Alert = {
        type: AlertType.Info,
        message: 'Context window',
        progress: { current: 50, total: 100 },
        showCompactButton: true,
        onCompact: mockOnCompact,
      };

      renderWithIntl(<AlertBox alert={alert} />);

      const compactButton = screen.getByText('Compact now');
      await user.click(compactButton);

      expect(mockOnCompact).toHaveBeenCalledTimes(1);
    });

    it('should prevent event propagation when compact button is clicked', () => {
      const mockParentClick = vi.fn();

      const alert: Alert = {
        type: AlertType.Info,
        message: 'Context window',
        progress: { current: 50, total: 100 },
        showCompactButton: true,
        onCompact: mockOnCompact,
      };

      renderWithIntl(
        <div onClick={mockParentClick}>
          <AlertBox alert={alert} />
        </div>
      );

      const compactButton = screen.getByText('Compact now');
      fireEvent.click(compactButton);

      expect(mockOnCompact).toHaveBeenCalledTimes(1);
      expect(mockParentClick).not.toHaveBeenCalled();
    });

    it('should not render compact button when showCompactButton is false', () => {
      const alert: Alert = {
        type: AlertType.Info,
        message: 'Context window',
        progress: { current: 50, total: 100 },
        showCompactButton: false,
        onCompact: mockOnCompact,
      };

      renderWithIntl(<AlertBox alert={alert} />);

      expect(screen.queryByText('Compact now')).not.toBeInTheDocument();
    });

    it('should not render compact button when onCompact is not provided', () => {
      const alert: Alert = {
        type: AlertType.Info,
        message: 'Context window',
        progress: { current: 50, total: 100 },
        showCompactButton: true,
      };

      renderWithIntl(<AlertBox alert={alert} />);

      expect(screen.queryByText('Compact now')).not.toBeInTheDocument();
    });
  });

  describe('Combined Features', () => {
    it('should render threshold settings and compact button together', async () => {
      const alert: Alert = {
        type: AlertType.Info,
        message: 'Context window',
        progress: {
          current: 75,
          total: 100,
        },
        showCompactButton: true,
        onCompact: mockOnCompact,
      };

      renderWithIntl(<AlertBox alert={alert} />);

      expect(await screen.findByText(/Auto compact at 80%/)).toBeInTheDocument();
      expect(screen.getByText('Compact now')).toBeInTheDocument();
    });

    it('should handle multiline messages', () => {
      const alert: Alert = {
        type: AlertType.Warning,
        message: 'Line 1\nLine 2\nLine 3',
      };

      renderWithIntl(<AlertBox alert={alert} />);

      expect(
        screen.getByText(
          (content) =>
            content.includes('Line 1') && content.includes('Line 2') && content.includes('Line 3')
        )
      ).toBeInTheDocument();
    });
  });

  describe('Edge Cases', () => {
    it('should handle empty message', () => {
      const alert: Alert = {
        type: AlertType.Info,
        message: '',
      };

      const { container } = renderWithIntl(<AlertBox alert={alert} />);

      const alertElement = container.querySelector('.flex.flex-col.gap-2');
      expect(alertElement).toBeInTheDocument();
    });

    it('should handle progress with zero total gracefully', async () => {
      const alert: Alert = {
        type: AlertType.Info,
        message: 'Context window',
        progress: {
          current: 10,
          total: 0,
        },
      };

      renderWithIntl(<AlertBox alert={alert} />);

      // Should still render threshold settings
      expect(await screen.findByText(/Auto compact at 80%/)).toBeInTheDocument();
    });
  });
});
