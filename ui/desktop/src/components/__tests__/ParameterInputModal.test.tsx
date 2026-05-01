import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, type RenderOptions, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import ParameterInputModal from '../ParameterInputModal';
import { IntlTestWrapper } from '../../i18n/test-utils';
import type { Parameter } from '../../recipe';

const renderWithIntl = (ui: React.ReactElement, options?: RenderOptions) =>
  render(ui, { wrapper: IntlTestWrapper, ...options });

const mockParameters: Parameter[] = [
  {
    key: 'param1',
    description: 'Test parameter 1',
    input_type: 'string',
    requirement: 'required',
  },
  {
    key: 'param2',
    description: 'Test parameter 2',
    input_type: 'select',
    requirement: 'optional',
    options: ['option1', 'option2'],
    default: 'option1',
  },
  {
    key: 'param3',
    description: 'Boolean parameter',
    input_type: 'boolean',
    requirement: 'optional',
    default: 'true',
  },
];

describe('ParameterInputModal', () => {
  const defaultProps = {
    parameters: mockParameters,
    onSubmit: vi.fn(),
    onClose: vi.fn(),
    initialValues: {},
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('Rendering', () => {
    it('renders modal with parameters', () => {
      renderWithIntl(<ParameterInputModal {...defaultProps} />);
      
      expect(screen.getByText('Recipe Parameters')).toBeInTheDocument();
      expect(screen.getByText('Test parameter 1')).toBeInTheDocument();
      expect(screen.getByText('Test parameter 2')).toBeInTheDocument();
      expect(screen.getByText('Boolean parameter')).toBeInTheDocument();
    });

    it('shows required indicator for required parameters', () => {
      renderWithIntl(<ParameterInputModal {...defaultProps} />);
      
      // param1 is required, should have red asterisk
      const requiredParam = screen.getByText('Test parameter 1');
      expect(requiredParam.parentElement?.querySelector('.text-red-500')).toBeInTheDocument();
    });
  });

  describe('Form Submission', () => {
    it('calls onSubmit with parameter values when submitted', async () => {
      const user = userEvent.setup();
      renderWithIntl(<ParameterInputModal {...defaultProps} />);

      await user.type(screen.getByLabelText(/test parameter 1/i), 'test value');
      await user.selectOptions(screen.getByLabelText(/test parameter 2/i), 'option2');
      
      // Submit form
      const submitButton = screen.getByText('Start Recipe');
      await user.click(submitButton);
      
      expect(defaultProps.onSubmit).toHaveBeenCalledWith({
        param1: 'test value',
        param2: 'option2',
        param3: 'true', // boolean default
      });
    });

    it('shows validation errors for required parameters', async () => {
      const user = userEvent.setup();
      renderWithIntl(<ParameterInputModal {...defaultProps} />);
      
      // Try to submit without filling required field
      const submitButton = screen.getByText('Start Recipe');
      await user.click(submitButton);
      
      await waitFor(() => {
        expect(screen.getByText(/is required/)).toBeInTheDocument();
      });
      expect(defaultProps.onSubmit).not.toHaveBeenCalled();
    });
  });

  describe('Cancel Behavior', () => {
    it('shows cancel options when cancel is clicked and parameters exist', async () => {
      const user = userEvent.setup();
      renderWithIntl(<ParameterInputModal {...defaultProps} />);
      
      const cancelButton = screen.getByText('Cancel');
      await user.click(cancelButton);
      
      expect(screen.getByText('Cancel Recipe Setup')).toBeInTheDocument();
      expect(screen.getByText('What would you like to do?')).toBeInTheDocument();
    });

    it('calls onClose directly when cancel is clicked and no parameters exist', async () => {
      const user = userEvent.setup();
      renderWithIntl(<ParameterInputModal {...defaultProps} parameters={[]} />);
      
      const cancelButton = screen.getByText('Cancel');
      await user.click(cancelButton);
      
      expect(defaultProps.onClose).toHaveBeenCalled();
    });

    it('calls onClose when "Start New Chat" option is selected', async () => {
      const user = userEvent.setup();
      renderWithIntl(<ParameterInputModal {...defaultProps} />);

      await user.click(screen.getByText('Cancel'));
      await user.click(screen.getByText('Start New Chat (No Recipe)'));

      expect(defaultProps.onClose).toHaveBeenCalledTimes(1);
    });

    it('returns to parameter form when "Back to Parameter Form" is clicked', async () => {
      const user = userEvent.setup();
      renderWithIntl(<ParameterInputModal {...defaultProps} />);
      
      // Click cancel to show options
      const cancelButton = screen.getByText('Cancel');
      await user.click(cancelButton);
      
      // Click "Back to Parameter Form"
      const backButton = screen.getByText('Back to Parameter Form');
      await user.click(backButton);
      
      // Should be back to parameter form
      expect(screen.getByText('Recipe Parameters')).toBeInTheDocument();
      expect(defaultProps.onClose).not.toHaveBeenCalled();
    });
  });

  describe('Initial Values', () => {
    it('pre-fills form with initial values', () => {
      renderWithIntl(
        <ParameterInputModal
          {...defaultProps}
          initialValues={{ param1: 'initial value' }}
        />
      );

      expect((screen.getByLabelText(/test parameter 1/i) as HTMLInputElement).value).toBe('initial value');
    });

    it('pre-fills form with default values from parameters', () => {
      renderWithIntl(<ParameterInputModal {...defaultProps} />);

      expect((screen.getByLabelText(/boolean parameter/i) as HTMLSelectElement).value).toBe('true');
    });
  });
});
