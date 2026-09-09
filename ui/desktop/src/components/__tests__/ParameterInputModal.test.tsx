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

      const submitButton = screen.getByText('Start Recipe');
      await user.click(submitButton);

      expect(defaultProps.onSubmit).toHaveBeenCalledWith({
        param1: 'test value',
        param2: 'option2',
        param3: 'true',
      });
    });

    it('prevents the default form submission when Enter is pressed in a parameter field', async () => {
      const user = userEvent.setup();
      renderWithIntl(<ParameterInputModal {...defaultProps} />);

      let submitFired = false;
      let defaultPrevented: boolean | undefined;
      const captureSubmit = (event: Event) => {
        submitFired = true;
        // Read before suppressing, otherwise this listener would mask the result
        defaultPrevented = event.defaultPrevented;
        event.preventDefault();
      };
      document.addEventListener('submit', captureSubmit);

      try {
        await user.type(screen.getByLabelText(/test parameter 1/i), 'test value{Enter}');
      } finally {
        document.removeEventListener('submit', captureSubmit);
      }

      expect(submitFired).toBe(true);
      expect(defaultPrevented).toBe(true);
    });

    it('shows validation errors for required parameters', async () => {
      const user = userEvent.setup();
      renderWithIntl(<ParameterInputModal {...defaultProps} />);

      const submitButton = screen.getByText('Start Recipe');
      await user.click(submitButton);

      await waitFor(() => {
        expect(screen.getByText(/is required/)).toBeInTheDocument();
      });
      expect(defaultProps.onSubmit).not.toHaveBeenCalled();
    });

    it('shows validation errors for user-prompt parameters', async () => {
      const user = userEvent.setup();
      renderWithIntl(
        <ParameterInputModal
          {...defaultProps}
          parameters={[
            {
              key: 'topic',
              description: 'Topic',
              input_type: 'string',
              requirement: 'user_prompt',
            },
          ]}
        />
      );

      const submitButton = screen.getByText('Start Recipe');
      await user.click(submitButton);

      await waitFor(() => {
        expect(screen.getByText('Topic is required')).toBeInTheDocument();
      });
      expect(defaultProps.onSubmit).not.toHaveBeenCalled();
    });

    it.each([
      {
        name: 'select value outside its options',
        parameter: {
          key: 'mode',
          description: 'Mode',
          input_type: 'select',
          requirement: 'required',
          options: ['safe'],
        } as Parameter,
        initialValue: 'hidden instruction',
        visibleValue: '',
      },
      {
        name: 'invalid boolean',
        parameter: {
          key: 'enabled',
          description: 'Enabled',
          input_type: 'boolean',
          requirement: 'required',
        } as Parameter,
        initialValue: 'hidden instruction',
        visibleValue: '',
      },
      {
        name: 'nonnumeric value',
        parameter: {
          key: 'iterations',
          description: 'Iterations',
          input_type: 'number',
          requirement: 'required',
        } as Parameter,
        initialValue: 'hidden instruction',
        visibleValue: null,
      },
      {
        name: 'number the browser cannot represent',
        parameter: {
          key: 'iterations',
          description: 'Iterations',
          input_type: 'number',
          requirement: 'required',
        } as Parameter,
        initialValue: '1.',
        visibleValue: null,
      },
    ])(
      'does not submit an invalid $name prefill',
      async ({ parameter, initialValue, visibleValue }) => {
        const user = userEvent.setup();
        const onSubmit = vi.fn();
        renderWithIntl(
          <ParameterInputModal
            {...defaultProps}
            onSubmit={onSubmit}
            parameters={[parameter]}
            initialValues={{ [parameter.key]: initialValue }}
          />
        );

        expect(screen.getByLabelText(new RegExp(`^${parameter.description}`))).toHaveValue(
          visibleValue
        );
        await user.click(screen.getByText('Start Recipe'));

        expect(onSubmit).not.toHaveBeenCalled();
      }
    );

    it('preserves free-text input for select parameters without options', async () => {
      const user = userEvent.setup();
      const onSubmit = vi.fn();
      renderWithIntl(
        <ParameterInputModal
          {...defaultProps}
          onSubmit={onSubmit}
          parameters={[
            {
              key: 'mode',
              description: 'Mode',
              input_type: 'select',
              requirement: 'required',
            },
          ]}
        />
      );

      await user.type(screen.getByLabelText(/^Mode/), 'custom value');
      await user.click(screen.getByText('Start Recipe'));

      expect(onSubmit).toHaveBeenCalledWith({ mode: 'custom value' });
    });

    it('submits only declared parameter keys', async () => {
      const user = userEvent.setup();
      const onSubmit = vi.fn();
      renderWithIntl(
        <ParameterInputModal
          {...defaultProps}
          onSubmit={onSubmit}
          parameters={[
            {
              key: 'topic',
              description: 'Topic',
              input_type: 'string',
              requirement: 'required',
            },
          ]}
          initialValues={{ topic: 'declared value', undeclared: 'hidden instruction' }}
        />
      );

      await user.click(screen.getByText('Start Recipe'));

      expect(onSubmit).toHaveBeenCalledWith({ topic: 'declared value' });
    });

    it.each([
      {
        name: 'select',
        parameter: {
          key: 'mode',
          description: 'Mode',
          input_type: 'select',
          requirement: 'optional',
          options: ['safe'],
          default: 'hidden instruction',
        } as Parameter,
      },
      {
        name: 'boolean',
        parameter: {
          key: 'enabled',
          description: 'Enabled',
          input_type: 'boolean',
          requirement: 'optional',
          default: 'TRUE',
        } as Parameter,
      },
      {
        name: 'number',
        parameter: {
          key: 'iterations',
          description: 'Iterations',
          input_type: 'number',
          requirement: 'optional',
          default: '1.',
        } as Parameter,
      },
    ])('blocks submission when an optional $name default is invalid', async ({ parameter }) => {
      const user = userEvent.setup();
      const onSubmit = vi.fn();
      renderWithIntl(
        <ParameterInputModal {...defaultProps} onSubmit={onSubmit} parameters={[parameter]} />
      );

      await user.click(screen.getByText('Start Recipe'));

      expect(screen.getByText(`${parameter.description} has an invalid value`)).toBeInTheDocument();
      expect(onSubmit).not.toHaveBeenCalled();
    });

    it('allows a valid user value to replace an invalid optional default', async () => {
      const user = userEvent.setup();
      const onSubmit = vi.fn();
      renderWithIntl(
        <ParameterInputModal
          {...defaultProps}
          onSubmit={onSubmit}
          parameters={[
            {
              key: 'mode',
              description: 'Mode',
              input_type: 'select',
              requirement: 'optional',
              options: ['safe'],
              default: 'hidden instruction',
            },
          ]}
        />
      );

      await user.selectOptions(screen.getByLabelText('Mode'), 'safe');
      await user.click(screen.getByText('Start Recipe'));

      expect(onSubmit).toHaveBeenCalledWith({ mode: 'safe' });
    });

    it.each(['string', 'date'] as const)(
      'submits an explicitly cleared optional %s value',
      async (inputType) => {
        const user = userEvent.setup();
        const onSubmit = vi.fn();
        renderWithIntl(
          <ParameterInputModal
            {...defaultProps}
            onSubmit={onSubmit}
            parameters={[
              {
                key: 'topic',
                description: 'Topic',
                input_type: inputType,
                requirement: 'optional',
                default: 'default topic',
              },
            ]}
          />
        );

        await user.clear(screen.getByLabelText('Topic'));
        await user.click(screen.getByText('Start Recipe'));

        expect(onSubmit).toHaveBeenCalledWith({ topic: '' });
      }
    );

    it('rejects an explicitly cleared optional controlled value', async () => {
      const user = userEvent.setup();
      const onSubmit = vi.fn();
      renderWithIntl(
        <ParameterInputModal
          {...defaultProps}
          onSubmit={onSubmit}
          parameters={[
            {
              key: 'mode',
              description: 'Mode',
              input_type: 'select',
              requirement: 'optional',
              options: ['safe'],
              default: 'safe',
            },
          ]}
        />
      );

      await user.selectOptions(screen.getByLabelText('Mode'), '');
      await user.click(screen.getByText('Start Recipe'));

      expect(screen.getByText('Mode has an invalid value')).toBeInTheDocument();
      expect(onSubmit).not.toHaveBeenCalled();
    });

    it.each(['__proto__', 'constructor', 'toString'])(
      'submits the reserved %s prefill as an own property',
      async (key) => {
        const user = userEvent.setup();
        const onSubmit = vi.fn();
        const initialValues = Object.fromEntries([[key, 'safe']]);
        renderWithIntl(
          <ParameterInputModal
            {...defaultProps}
            onSubmit={onSubmit}
            parameters={[
              {
                key,
                description: 'Reserved parameter',
                input_type: 'string',
                requirement: 'required',
              },
            ]}
            initialValues={initialValues}
          />
        );

        expect(screen.getByLabelText(/^Reserved parameter/)).toHaveValue('safe');
        await user.click(screen.getByText('Start Recipe'));

        const submittedValues = onSubmit.mock.calls[0][0];
        expect(Object.prototype.hasOwnProperty.call(submittedValues, key)).toBe(true);
        expect(submittedValues[key]).toBe('safe');
      }
    );

    it('submits an entered __proto__ value as an own property', async () => {
      const user = userEvent.setup();
      const onSubmit = vi.fn();
      renderWithIntl(
        <ParameterInputModal
          {...defaultProps}
          onSubmit={onSubmit}
          parameters={[
            {
              key: '__proto__',
              description: 'Prototype parameter',
              input_type: 'string',
              requirement: 'required',
            },
          ]}
        />
      );

      await user.type(screen.getByLabelText(/^Prototype parameter/), 'safe');
      await user.click(screen.getByText('Start Recipe'));

      const submittedValues = onSubmit.mock.calls[0][0];
      expect(Object.prototype.hasOwnProperty.call(submittedValues, '__proto__')).toBe(true);
      expect(submittedValues.__proto__).toBe('safe');
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

      const cancelButton = screen.getByText('Cancel');
      await user.click(cancelButton);

      const backButton = screen.getByText('Back to Parameter Form');
      await user.click(backButton);

      expect(screen.getByText('Recipe Parameters')).toBeInTheDocument();
      expect(defaultProps.onClose).not.toHaveBeenCalled();
    });
  });

  describe('Initial Values', () => {
    it('pre-fills form with initial values', () => {
      renderWithIntl(
        <ParameterInputModal {...defaultProps} initialValues={{ param1: 'initial value' }} />
      );

      expect((screen.getByLabelText(/test parameter 1/i) as HTMLInputElement).value).toBe(
        'initial value'
      );
    });

    it('pre-fills form with default values from parameters', () => {
      renderWithIntl(<ParameterInputModal {...defaultProps} />);

      expect((screen.getByLabelText(/boolean parameter/i) as HTMLSelectElement).value).toBe('true');
    });

    it('does not initialize file parameters from prefills or defaults', async () => {
      const user = userEvent.setup();
      const onSubmit = vi.fn();
      renderWithIntl(
        <ParameterInputModal
          {...defaultProps}
          onSubmit={onSubmit}
          parameters={[
            {
              key: 'document',
              description: 'Document',
              input_type: 'file',
              requirement: 'required',
              default: '/tmp/default.txt',
            },
            {
              key: 'topic',
              description: 'Topic',
              input_type: 'string',
              requirement: 'required',
            },
          ]}
          initialValues={{ document: '/tmp/private.txt', topic: 'release notes' }}
        />
      );

      expect(screen.getByLabelText(/^Document/)).toHaveValue('');
      expect(screen.getByLabelText(/^Topic/)).toHaveValue('release notes');

      await user.type(screen.getByLabelText(/^Document/), '/tmp/selected.txt');
      await user.click(screen.getByText('Start Recipe'));

      expect(onSubmit).toHaveBeenCalledWith({
        document: '/tmp/selected.txt',
        topic: 'release notes',
      });
    });

    it('keeps a valid default when an invalid prefill is supplied', () => {
      renderWithIntl(
        <ParameterInputModal
          {...defaultProps}
          parameters={[
            {
              key: 'mode',
              description: 'Mode',
              input_type: 'select',
              requirement: 'optional',
              options: ['safe'],
              default: 'safe',
            },
          ]}
          initialValues={{ mode: 'hidden instruction' }}
        />
      );

      expect(screen.getByLabelText('Mode')).toHaveValue('safe');
    });

    it('submits valid select, boolean, and number prefills', async () => {
      const user = userEvent.setup();
      const onSubmit = vi.fn();
      renderWithIntl(
        <ParameterInputModal
          {...defaultProps}
          onSubmit={onSubmit}
          parameters={[
            {
              key: 'mode',
              description: 'Mode',
              input_type: 'select',
              requirement: 'required',
              options: ['safe'],
            },
            {
              key: 'enabled',
              description: 'Enabled',
              input_type: 'boolean',
              requirement: 'required',
            },
            {
              key: 'iterations',
              description: 'Iterations',
              input_type: 'number',
              requirement: 'required',
            },
          ]}
          initialValues={{ mode: 'safe', enabled: 'false', iterations: '1.5e2' }}
        />
      );

      expect(screen.getByLabelText(/^Mode/)).toHaveValue('safe');
      expect(screen.getByLabelText(/^Enabled/)).toHaveValue('false');
      expect(screen.getByLabelText(/^Iterations/)).toHaveValue(150);

      await user.click(screen.getByText('Start Recipe'));

      expect(onSubmit).toHaveBeenCalledWith({
        mode: 'safe',
        enabled: 'false',
        iterations: '1.5e2',
      });
    });
  });
});
