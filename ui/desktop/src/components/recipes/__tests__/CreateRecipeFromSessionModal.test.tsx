import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, type RenderOptions, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import CreateRecipeFromSessionModal from '../CreateRecipeFromSessionModal';
import { createRecipe } from '../../../api/sdk.gen';
import type { CreateRecipeResponse } from '../../../api/types.gen';
import { IntlTestWrapper } from '../../../i18n/test-utils';

const renderWithIntl = (ui: React.ReactElement, options?: RenderOptions) =>
  render(ui, { wrapper: IntlTestWrapper, ...options });

vi.mock('../../../api/sdk.gen', () => ({
  createRecipe: vi.fn(),
}));

vi.mock('../../../toasts', () => ({
  toastError: vi.fn(),
}));

vi.mock('../../../recipe/recipe_management', () => ({
  saveRecipe: vi.fn().mockResolvedValue({ id: 'mock-recipe-id', fileName: 'mock-recipe.yaml' }),
}));

vi.mock('../../ConfigContext', () => ({
  useConfig: () => ({
    extensionsList: [],
    getExtensions: vi.fn().mockResolvedValue([]),
    getProviders: vi.fn().mockResolvedValue([]),
  }),
}));

const mockCreateRecipe = vi.mocked(createRecipe);

describe('CreateRecipeFromSessionModal', () => {
  const defaultProps = {
    isOpen: true,
    onClose: vi.fn(),
    sessionId: 'test-session-id',
    onRecipeCreated: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
    const mockResponse: CreateRecipeResponse = {
      recipe: {
        title: 'Analyzed Recipe Title',
        description: 'Analyzed description',
        instructions: 'Analyzed instructions with {{param1}}',
        prompt: 'Analyzed prompt',
        activities: ['activity1', 'activity2'],
        parameters: [
          {
            key: 'param1',
            description: 'Auto-detected parameter',
            input_type: 'string',
            requirement: 'required',
          },
        ],
        response: {
          json_schema: { type: 'object' },
        },
      },
      error: undefined,
    };

    mockCreateRecipe.mockResolvedValue({
      data: mockResponse,
      error: undefined,
      request: new globalThis.Request('http://localhost/test'),
      response: new globalThis.Response(),
    });
  });

  describe('Modal Rendering', () => {
    it('renders modal when open', () => {
      renderWithIntl(<CreateRecipeFromSessionModal {...defaultProps} />);

      expect(screen.getByTestId('create-recipe-modal')).toBeInTheDocument();
    });

    it('does not render when closed', () => {
      renderWithIntl(<CreateRecipeFromSessionModal {...defaultProps} isOpen={false} />);

      expect(screen.queryByTestId('create-recipe-modal')).not.toBeInTheDocument();
    });

    it('renders modal header with close button', () => {
      renderWithIntl(<CreateRecipeFromSessionModal {...defaultProps} />);

      expect(screen.getByTestId('modal-header')).toBeInTheDocument();
      expect(screen.getByTestId('close-button')).toBeInTheDocument();
    });

    it('calls onClose when close button is clicked', async () => {
      const user = userEvent.setup();
      renderWithIntl(<CreateRecipeFromSessionModal {...defaultProps} />);

      await user.click(screen.getByTestId('close-button'));
      expect(defaultProps.onClose).toHaveBeenCalled();
    });
  });

  describe('Analysis Workflow', () => {
    it('shows analyzing state initially', () => {
      renderWithIntl(<CreateRecipeFromSessionModal {...defaultProps} />);

      expect(screen.getByTestId('analyzing-state')).toBeInTheDocument();
      expect(screen.getByTestId('analyzing-title')).toBeInTheDocument();
    });

    it('displays analysis progress indicator', async () => {
      renderWithIntl(<CreateRecipeFromSessionModal {...defaultProps} />);

      expect(screen.getByTestId('analysis-stage')).toBeInTheDocument();

      await waitFor(
        () => {
          const stageElement = screen.getByTestId('analysis-stage');
          expect(stageElement).toBeInTheDocument();
        },
        { timeout: 1000 }
      );
    });

    it('shows loading indicator during analysis', () => {
      renderWithIntl(<CreateRecipeFromSessionModal {...defaultProps} />);

      expect(screen.getByTestId('analysis-spinner')).toBeInTheDocument();
    });

    it('transitions to form state after analysis completes', async () => {
      renderWithIntl(<CreateRecipeFromSessionModal {...defaultProps} />);

      await waitFor(
        () => {
          expect(screen.getByTestId('form-state')).toBeInTheDocument();
        },
        { timeout: 3000 }
      );

      expect(screen.queryByTestId('analyzing-state')).not.toBeInTheDocument();
    });
  });

  describe('Form Pre-filling', () => {
    it('pre-fills form with analyzed data', async () => {
      renderWithIntl(<CreateRecipeFromSessionModal {...defaultProps} />);

      // Wait for analysis to complete and form to be pre-filled
      await waitFor(
        () => {
          expect(screen.getByDisplayValue('Analyzed Recipe Title')).toBeInTheDocument();
        },
        { timeout: 2000 }
      );

      expect(screen.getByDisplayValue('Analyzed description')).toBeInTheDocument();
      expect(screen.getByDisplayValue('Analyzed instructions with {{param1}}')).toBeInTheDocument();
      const promptInput = screen.getByTestId('prompt-input');
      expect(promptInput).toBeInTheDocument();
    });

    it('shows recipe form fields after analysis', async () => {
      renderWithIntl(<CreateRecipeFromSessionModal {...defaultProps} />);

      await waitFor(
        () => {
          expect(screen.getByTestId('recipe-form')).toBeInTheDocument();
        },
        { timeout: 2000 }
      );

      expect(screen.getByTestId('title-input')).toBeInTheDocument();
      expect(screen.getByTestId('description-input')).toBeInTheDocument();
      expect(screen.getByTestId('instructions-input')).toBeInTheDocument();
      expect(screen.getByTestId('prompt-input')).toBeInTheDocument();
    });
  });

  describe('Form Interactions', () => {
    it('allows editing form fields', async () => {
      const user = userEvent.setup();
      renderWithIntl(<CreateRecipeFromSessionModal {...defaultProps} />);

      await waitFor(
        () => {
          expect(screen.getByTestId('title-input')).toBeInTheDocument();
        },
        { timeout: 2000 }
      );

      const titleInput = screen.getByTestId('title-input');
      await user.clear(titleInput);
      await user.type(titleInput, 'Modified Title');

      expect(screen.getByDisplayValue('Modified Title')).toBeInTheDocument();
    });

    it('validates required fields', async () => {
      const user = userEvent.setup();
      renderWithIntl(<CreateRecipeFromSessionModal {...defaultProps} />);

      await waitFor(
        () => {
          expect(screen.getByTestId('create-recipe-button')).toBeInTheDocument();
        },
        { timeout: 2000 }
      );

      const titleInput = screen.getByTestId('title-input');
      await user.clear(titleInput);

      const createButton = screen.getByTestId('create-recipe-button');
      expect(createButton).toBeDisabled();
    });
  });

  describe('Recipe Creation', () => {
    it('enables create button when form is valid', async () => {
      renderWithIntl(<CreateRecipeFromSessionModal {...defaultProps} />);

      await waitFor(
        () => {
          const createButton = screen.getByTestId('create-recipe-button');
          expect(createButton).toBeEnabled();
        },
        { timeout: 2000 }
      );
    });

    it('creates recipe and closes modal when form is submitted', async () => {
      const user = userEvent.setup();
      renderWithIntl(<CreateRecipeFromSessionModal {...defaultProps} />);

      await waitFor(
        () => {
          expect(screen.getByTestId('create-recipe-button')).toBeEnabled();
        },
        { timeout: 2000 }
      );

      await user.click(screen.getByTestId('create-recipe-button'));

      await waitFor(() => {
        expect(defaultProps.onRecipeCreated).toHaveBeenCalled();
        expect(defaultProps.onClose).toHaveBeenCalled();
      });
    });
  });

  describe('Modal Footer', () => {
    it('shows cancel button in all states', async () => {
      renderWithIntl(<CreateRecipeFromSessionModal {...defaultProps} />);

      expect(screen.getByTestId('cancel-button')).toBeInTheDocument();

      await waitFor(
        () => {
          expect(screen.getByTestId('create-recipe-button')).toBeInTheDocument();
        },
        { timeout: 2000 }
      );

      expect(screen.getByTestId('cancel-button')).toBeInTheDocument();
    });

    it('calls onClose when cancel button is clicked', async () => {
      const user = userEvent.setup();
      renderWithIntl(<CreateRecipeFromSessionModal {...defaultProps} />);

      await user.click(screen.getByTestId('cancel-button'));
      expect(defaultProps.onClose).toHaveBeenCalled();
    });

    it('shows different button states based on workflow stage', async () => {
      renderWithIntl(<CreateRecipeFromSessionModal {...defaultProps} />);

      expect(screen.getByTestId('cancel-button')).toBeInTheDocument();
      expect(screen.queryByTestId('create-recipe-button')).not.toBeInTheDocument();

      await waitFor(
        () => {
          expect(screen.getByTestId('create-recipe-button')).toBeInTheDocument();
        },
        { timeout: 2000 }
      );

      expect(screen.getByTestId('create-and-run-recipe-button')).toBeInTheDocument();
    });
  });

  describe('Error Handling', () => {
    it('handles analysis errors gracefully', async () => {
      renderWithIntl(<CreateRecipeFromSessionModal {...defaultProps} sessionId="" />);

      expect(screen.getByTestId('create-recipe-modal')).toBeInTheDocument();
    });

    it('handles form validation errors', async () => {
      const user = userEvent.setup();
      renderWithIntl(<CreateRecipeFromSessionModal {...defaultProps} />);

      await waitFor(
        () => {
          expect(screen.getByTestId('title-input')).toBeInTheDocument();
        },
        { timeout: 2000 }
      );

      await user.clear(screen.getByTestId('title-input'));
      await user.clear(screen.getByTestId('description-input'));
      await user.clear(screen.getByTestId('instructions-input'));

      const createButton = screen.getByTestId('create-recipe-button');
      expect(createButton).toBeDisabled();
    });
  });
});
