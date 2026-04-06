import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, type RenderOptions, screen, fireEvent } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { AlertBox } from '../AlertBox';
import { Alert, AlertType } from '../types';
import { IntlTestWrapper } from '../../../i18n/test-utils';

const renderWithIntl = (ui: React.ReactElement, options?: RenderOptions) =>
  render(ui, { wrapper: IntlTestWrapper, ...options });

// Mock the ConfigContext
vi.mock('../../ConfigContext', () => ({
  useConfig: () => ({
    read: vi.fn().mockResolvedValue(0.8),
  }),
}));

describe('AlertBox', () => {
  const mockOnCompact = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
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

  describe('Progress Bar', () => {
    it('should render progress bar when progress is provided', () => {
      const alert: Alert = {
        type: AlertType.Info,
        message: 'Context window',
        progress: {
          current: 50,
          total: 100,
        },
      };

      renderWithIntl(<AlertBox alert={alert} />);

      expect(screen.getByText('50')).toBeInTheDocument();
      expect(screen.getByText('50%')).toBeInTheDocument();
      expect(screen.getByText('100')).toBeInTheDocument();

      // Check progress bar exists
      const progressDots = screen
        .getByText('Context window')
        .parentElement?.parentElement?.querySelectorAll('.h-\\[2px\\]');
      expect(progressDots).toBeDefined();
    });

    it('should handle zero current value', () => {
      const alert: Alert = {
        type: AlertType.Info,
        message: 'Context window',
        progress: {
          current: 0,
          total: 100,
        },
      };

      renderWithIntl(<AlertBox alert={alert} />);

      expect(screen.getByText('0')).toBeInTheDocument();
      expect(screen.getByText('0%')).toBeInTheDocument();
      expect(screen.getByText('100')).toBeInTheDocument();
    });

    it('should handle 100% progress', () => {
      const alert: Alert = {
        type: AlertType.Info,
        message: 'Context window',
        progress: {
          current: 100,
          total: 100,
        },
      };

      renderWithIntl(<AlertBox alert={alert} />);

      // Use getAllByText since there are multiple "100" elements (current and total)
      const hundredElements = screen.getAllByText('100');
      expect(hundredElements).toHaveLength(2); // One for current, one for total
      expect(screen.getByText('100%')).toBeInTheDocument();
    });

    it('should format large numbers with k suffix', () => {
      const alert: Alert = {
        type: AlertType.Info,
        message: 'Context window',
        progress: {
          current: 1500,
          total: 10000,
        },
      };

      renderWithIntl(<AlertBox alert={alert} />);

      expect(screen.getByText('1.5k')).toBeInTheDocument();
      expect(screen.getByText('15%')).toBeInTheDocument();
      expect(screen.getByText('10k')).toBeInTheDocument();
    });

    it('should handle progress over 100%', () => {
      const alert: Alert = {
        type: AlertType.Warning,
        message: 'Context window',
        progress: {
          current: 150,
          total: 100,
        },
      };

      renderWithIntl(<AlertBox alert={alert} />);

      expect(screen.getByText('150')).toBeInTheDocument();
      expect(screen.getByText('150%')).toBeInTheDocument();
      expect(screen.getByText('100')).toBeInTheDocument();
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
    it('should render progress bar and compact button together', () => {
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

      expect(screen.getByText('75')).toBeInTheDocument();
      expect(screen.getByText('75%')).toBeInTheDocument();
      expect(screen.getByText('100')).toBeInTheDocument();
      expect(screen.getByText('Compact now')).toBeInTheDocument();
    });

    it('should handle multiline messages', () => {
      const alert: Alert = {
        type: AlertType.Warning,
        message: 'Line 1\nLine 2\nLine 3',
      };

      renderWithIntl(<AlertBox alert={alert} />);

      // Use a function matcher to handle the whitespace-pre-line rendering
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

      // Should still render the alert container
      const alertElement = container.querySelector('.flex.flex-col.gap-2');
      expect(alertElement).toBeInTheDocument();
    });

    it('should handle progress with zero total', () => {
      const alert: Alert = {
        type: AlertType.Info,
        message: 'Context window',
        progress: {
          current: 10,
          total: 0,
        },
      };

      renderWithIntl(<AlertBox alert={alert} />);

      expect(screen.getByText('10')).toBeInTheDocument();
      expect(screen.getByText('0')).toBeInTheDocument();
      // Progress percentage would be Infinity, but it should still render
      expect(screen.getByText('Infinity%')).toBeInTheDocument();
    });
  });
});
