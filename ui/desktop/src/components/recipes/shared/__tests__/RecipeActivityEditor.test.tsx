import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, type RenderOptions, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import RecipeActivityEditor from '../../RecipeActivityEditor';
import { IntlTestWrapper } from '../../../../i18n/test-utils';

const renderWithIntl = (ui: React.ReactElement, options?: RenderOptions) =>
  render(ui, { wrapper: IntlTestWrapper, ...options });

describe('RecipeActivityEditor', () => {
  const mockOnChange = vi.fn();
  const mockOnBlur = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('Basic Rendering', () => {
    it('renders without crashing', () => {
      renderWithIntl(<RecipeActivityEditor activities={[]} setActivities={mockOnChange} />);
      expect(screen.getByText('Activities')).toBeInTheDocument();
    });

    it('displays the activities label', () => {
      renderWithIntl(<RecipeActivityEditor activities={[]} setActivities={mockOnChange} />);
      expect(screen.getByText('Activities')).toBeInTheDocument();
    });

    it('shows helper text', () => {
      renderWithIntl(<RecipeActivityEditor activities={[]} setActivities={mockOnChange} />);
      expect(screen.getByText(/top-line prompts and activity buttons/)).toBeInTheDocument();
    });
  });

  describe('Empty State', () => {
    it('shows message input when no activities', () => {
      renderWithIntl(<RecipeActivityEditor activities={[]} setActivities={mockOnChange} />);
      expect(screen.getByText('Message')).toBeInTheDocument();
      expect(
        screen.getByPlaceholderText(/Enter a user facing introduction message/)
      ).toBeInTheDocument();
    });
  });

  describe('With Activities', () => {
    it('displays existing activities as visual boxes', () => {
      const activities = ['message: Hello World', 'button: Click me', 'action: Do something'];
      renderWithIntl(<RecipeActivityEditor activities={activities} setActivities={mockOnChange} />);

      const messageTextarea = screen.getByPlaceholderText(
        /Enter a user facing introduction message/
      );
      expect(messageTextarea).toHaveValue(' Hello World');

      expect(screen.getByText('button: Click me')).toBeInTheDocument();
      expect(screen.getByText('action: Do something')).toBeInTheDocument();

      const removeButtons = screen.getAllByText('×');
      expect(removeButtons).toHaveLength(2);
    });

    it('truncates long activity text in boxes', () => {
      const longActivity = 'button: ' + 'a'.repeat(150);
      const activities = [longActivity];
      renderWithIntl(<RecipeActivityEditor activities={activities} setActivities={mockOnChange} />);

      expect(screen.getByText(/button: a+\.\.\./)).toBeInTheDocument();

      const activityBox = screen.getByText(/button: a+\.\.\./).closest('div');
      expect(activityBox).toHaveAttribute('title', longActivity);
    });

    it('handles empty activities array', () => {
      renderWithIntl(<RecipeActivityEditor activities={[]} setActivities={mockOnChange} />);
      expect(screen.getByText('Activities')).toBeInTheDocument();

      expect(screen.queryByText('×')).not.toBeInTheDocument();
    });

    it('allows removing activities via remove buttons', async () => {
      const user = userEvent.setup();
      const activities = ['button: Click me', 'action: Do something'];
      renderWithIntl(<RecipeActivityEditor activities={activities} setActivities={mockOnChange} />);

      const removeButtons = screen.getAllByText('×');
      await user.click(removeButtons[0]);

      expect(mockOnChange).toHaveBeenCalledWith(['action: Do something']);
    });
  });

  describe('User Interactions', () => {
    it('allows typing in message field', async () => {
      const user = userEvent.setup();
      renderWithIntl(<RecipeActivityEditor activities={[]} setActivities={mockOnChange} />);

      const messageInput = screen.getByPlaceholderText(/Enter a user facing introduction message/);
      await user.type(messageInput, 'Test message');

      expect(messageInput).toHaveValue('Test message');
    });

    it('calls onBlur when provided', async () => {
      const user = userEvent.setup();
      renderWithIntl(
        <RecipeActivityEditor activities={[]} setActivities={mockOnChange} onBlur={mockOnBlur} />
      );

      const messageInput = screen.getByPlaceholderText(/Enter a user facing introduction message/);
      await user.click(messageInput);
      await user.tab();

      expect(mockOnBlur).toHaveBeenCalled();
    });
  });
});
