import { useState, useEffect } from 'react';
import { useForm } from '@tanstack/react-form';
import { Recipe } from '../../recipe';
import { Geese } from '../icons/Geese';
import { X, Save, Play, Loader2 } from 'lucide-react';
import { Button } from '../ui/button';
import { RecipeFormFields } from './shared/RecipeFormFields';
import { RecipeFormData } from './shared/recipeFormSchema';
import { createRecipe } from '../../api/sdk.gen';
import { RecipeParameter } from './shared/recipeFormSchema';
import { toastError } from '../../toasts';
import { saveRecipe } from '../../recipe/recipe_management';
import { errorMessage } from '../../utils/conversionUtils';
import { defineMessages, useIntl } from '../../i18n';

const i18n = defineMessages({
  title: {
    id: 'createRecipeFromSession.title',
    defaultMessage: 'Create Recipe from Session',
  },
  subtitle: {
    id: 'createRecipeFromSession.subtitle',
    defaultMessage: 'Create a reusable recipe based on your current conversation.',
  },
  analyzingTitle: {
    id: 'createRecipeFromSession.analyzingTitle',
    defaultMessage: 'Analyzing your conversation',
  },
  stageReading: {
    id: 'createRecipeFromSession.stageReading',
    defaultMessage: 'Reading your conversation...',
  },
  stageIdentifying: {
    id: 'createRecipeFromSession.stageIdentifying',
    defaultMessage: 'Identifying key patterns...',
  },
  stageExtracting: {
    id: 'createRecipeFromSession.stageExtracting',
    defaultMessage: 'Extracting main topics...',
  },
  stageGenerating: {
    id: 'createRecipeFromSession.stageGenerating',
    defaultMessage: 'Generating recipe structure...',
  },
  stageFinalizing: {
    id: 'createRecipeFromSession.stageFinalizing',
    defaultMessage: 'Finalizing details...',
  },
  stageComplete: {
    id: 'createRecipeFromSession.stageComplete',
    defaultMessage: 'Complete!',
  },
  extractingInsights: {
    id: 'createRecipeFromSession.extractingInsights',
    defaultMessage: 'Extracting insights from your chat',
  },
  cancel: {
    id: 'createRecipeFromSession.cancel',
    defaultMessage: 'Cancel',
  },
  creating: {
    id: 'createRecipeFromSession.creating',
    defaultMessage: 'Creating...',
  },
  createRecipe: {
    id: 'createRecipeFromSession.createRecipe',
    defaultMessage: 'Create Recipe',
  },
  createAndRunRecipe: {
    id: 'createRecipeFromSession.createAndRunRecipe',
    defaultMessage: 'Create & Run Recipe',
  },
  failedToCreateTitle: {
    id: 'createRecipeFromSession.failedToCreateTitle',
    defaultMessage: 'Failed to create recipe',
  },
  failedToCreateDefaultMsg: {
    id: 'createRecipeFromSession.failedToCreateDefaultMsg',
    defaultMessage: 'An unexpected error occurred while creating the recipe. Please try again.',
  },
});

interface CreateRecipeFromSessionModalProps {
  isOpen: boolean;
  onClose: () => void;
  sessionId: string;
  onRecipeCreated?: (recipe: Recipe) => void;
}

export default function CreateRecipeFromSessionModal({
  isOpen,
  onClose,
  sessionId,
  onRecipeCreated,
}: CreateRecipeFromSessionModalProps) {
  const intl = useIntl();
  const [isCreating, setIsCreating] = useState(false);
  const [isAnalyzing, setIsAnalyzing] = useState(false);
  const [analysisStage, setAnalysisStage] = useState<string>('');
  const [hasAnalyzed, setHasAnalyzed] = useState(false);

  // Initialize form with empty values for new recipe
  const form = useForm({
    defaultValues: {
      title: '',
      description: '',
      instructions: '',
      prompt: '',
      activities: [] as string[],
      parameters: [] as RecipeParameter[],
      jsonSchema: '',
      subRecipes: [],
      recipeName: '',
      global: true,
    } as RecipeFormData,
    onSubmit: async ({ value }) => {
      await handleCreateRecipe(value);
    },
  });

  // Track form validity with state to make it reactive
  const [isFormValid, setIsFormValid] = useState(false);

  // Analyze messages and prefill form when modal opens
  useEffect(() => {
    if (isOpen && sessionId && !hasAnalyzed) {
      setIsAnalyzing(true);

      // Create a sequence of analysis stages for better UX
      const stages = [
        intl.formatMessage(i18n.stageReading),
        intl.formatMessage(i18n.stageIdentifying),
        intl.formatMessage(i18n.stageExtracting),
        intl.formatMessage(i18n.stageGenerating),
        intl.formatMessage(i18n.stageFinalizing),
      ];

      let currentStageIndex = 0;
      setAnalysisStage(stages[0]);

      // Update stage every 800ms
      const stageInterval = setInterval(() => {
        currentStageIndex = (currentStageIndex + 1) % stages.length;
        setAnalysisStage(stages[currentStageIndex]);
      }, 800);

      // Call the backend to analyze messages and create a recipe
      createRecipe({
        body: { session_id: sessionId },
        throwOnError: true,
      })
        .then((response) => {
          clearInterval(stageInterval);
          setAnalysisStage(intl.formatMessage(i18n.stageComplete));

          if (response.data?.recipe) {
            const recipe = response.data.recipe;

            // Prefill the form with the analyzed recipe information
            form.setFieldValue('title', recipe.title || '');
            form.setFieldValue('description', recipe.description || '');
            form.setFieldValue('instructions', recipe.instructions || '');
            form.setFieldValue('activities', recipe.activities || []);
            form.setFieldValue('parameters', recipe.parameters || []);

            if (recipe.response?.json_schema) {
              form.setFieldValue(
                'jsonSchema',
                JSON.stringify(recipe.response.json_schema, null, 2)
              );
            }
          } else {
            console.error('No recipe in response:', response);
          }
          setHasAnalyzed(true);
        })
        .catch((error) => {
          console.error('Failed to analyze messages:', error);
          setAnalysisStage('Analysis failed');
        })
        .finally(() => {
          clearInterval(stageInterval);
          setHasAnalyzed(true);
          setTimeout(() => {
            setIsAnalyzing(false);
            setAnalysisStage('');
          }, 500); // Brief delay to show completion
        });
    }
  }, [isOpen, sessionId, hasAnalyzed, form, intl]);

  // Reset analysis state when modal closes
  useEffect(() => {
    if (!isOpen) {
      setHasAnalyzed(false);
      setIsAnalyzing(false);
      setAnalysisStage('');
    }
  }, [isOpen]);

  // Subscribe to form changes using the form's subscribe method
  useEffect(() => {
    const unsubscribe = form.store.subscribe(() => {
      const hasTitle = form.state.values.title?.trim();
      const hasDescription = form.state.values.description?.trim();
      const hasInstructions = form.state.values.instructions?.trim();
      const valid = !!(hasTitle && hasDescription && hasInstructions);

      setIsFormValid(valid);
    });

    // Initial validation check
    const hasTitle = form.state.values.title?.trim();
    const hasDescription = form.state.values.description?.trim();
    const hasInstructions = form.state.values.instructions?.trim();
    const valid = !!(hasTitle && hasDescription && hasInstructions);
    setIsFormValid(valid);

    return unsubscribe;
  }, [form]);

  const handleCreateRecipe = async (formData: RecipeFormData, runAfterSave = false) => {
    if (!isFormValid) {
      return;
    }

    setIsCreating(true);
    try {
      const formattedSubRecipes =
        formData.subRecipes.length > 0
          ? formData.subRecipes.map((subRecipe) => ({
              name: subRecipe.name,
              path: subRecipe.path,
              description: subRecipe.description || undefined,
              values:
                subRecipe.values && Object.keys(subRecipe.values).length > 0
                  ? subRecipe.values
                  : undefined,
              sequential_when_repeated: subRecipe.sequential_when_repeated,
            }))
          : undefined;

      const recipe: Recipe = {
        title: formData.title,
        description: formData.description,
        instructions: formData.instructions,
        prompt: formData.prompt || undefined,
        activities: formData.activities.filter((activity) => activity.trim() !== ''),
        parameters: formData.parameters.map((param) => ({
          key: param.key,
          input_type: param.input_type || 'string',
          requirement: param.requirement,
          description: param.description,
          ...(param.requirement === 'optional' && param.default ? { default: param.default } : {}),
          ...(param.input_type === 'select' && param.options
            ? {
                options: param.options.filter((opt: string) => opt.trim() !== ''),
              }
            : {}),
        })),
        response:
          formData.jsonSchema && formData.jsonSchema.trim()
            ? {
                json_schema: JSON.parse(formData.jsonSchema),
              }
            : undefined,
        sub_recipes: formattedSubRecipes,
      };

      const { id: recipeId } = await saveRecipe(recipe, null);

      onRecipeCreated?.(recipe);
      onClose();

      if (runAfterSave) {
        window.electron.createChatWindow({ recipeId });
      }
    } catch (error) {
      console.error('Failed to create recipe:', error);
      toastError({
        title: intl.formatMessage(i18n.failedToCreateTitle),
        msg: errorMessage(
          error,
          intl.formatMessage(i18n.failedToCreateDefaultMsg)
        ),
      });
    } finally {
      setIsCreating(false);
    }
  };

  if (!isOpen) return null;

  return (
    <div
      className="fixed inset-0 z-[400] flex items-center justify-center bg-black/50 p-4"
      data-testid="create-recipe-modal"
    >
      <div className="bg-background-primary border border-border-primary rounded-lg w-full max-w-4xl h-full max-h-[90vh] flex flex-col shadow-xl">
        {/* Header */}
        <div
          className="flex items-center justify-between p-6 border-b border-border-primary shrink-0"
          data-testid="modal-header"
        >
          <div className="flex items-center gap-3">
            <div className="w-8 h-8 bg-background-primary rounded-full flex items-center justify-center">
              <Geese className="w-6 h-6 text-iconProminent" />
            </div>
            <div>
              <h1 className="text-xl font-medium text-text-primary">{intl.formatMessage(i18n.title)}</h1>
              <p className="text-text-secondary text-sm">
                {intl.formatMessage(i18n.subtitle)}
              </p>
            </div>
          </div>
          <Button
            onClick={onClose}
            variant="ghost"
            size="sm"
            className="p-2 hover:bg-background-secondary rounded-lg transition-colors"
            data-testid="close-button"
          >
            <X className="w-5 h-5" />
          </Button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto px-6 py-4 min-h-0" data-testid="modal-content">
          {isAnalyzing ? (
            <div
              className="flex flex-col items-center justify-center h-full min-h-[300px] space-y-4"
              data-testid="analyzing-state"
            >
              <div className="flex items-center space-x-3">
                <Loader2
                  className="w-6 h-6 animate-spin text-iconProminent"
                  data-testid="analysis-spinner"
                />
                <div
                  className="text-lg font-medium text-text-primary"
                  data-testid="analyzing-title"
                >
                  {intl.formatMessage(i18n.analyzingTitle)}
                </div>
              </div>
              <div
                className="text-text-secondary text-center max-w-md"
                data-testid="analysis-stage"
              >
                {analysisStage}
              </div>
              <div className="flex items-center space-x-2 text-text-secondary">
                <Geese className="w-5 h-5 animate-pulse" />
                <span className="text-sm">{intl.formatMessage(i18n.extractingInsights)}</span>
              </div>
            </div>
          ) : (
            <div data-testid="form-state">
              <RecipeFormFields form={form} />
            </div>
          )}
        </div>

        {/* Footer */}
        <div
          className="flex items-center justify-between p-6 border-t border-border-primary shrink-0"
          data-testid="modal-footer"
        >
          <Button
            onClick={onClose}
            variant="ghost"
            className="px-4 py-2 text-text-secondary rounded-lg hover:bg-background-secondary transition-colors"
            data-testid="cancel-button"
          >
            {intl.formatMessage(i18n.cancel)}
          </Button>

          <div className="flex gap-3">
            {!isAnalyzing && (
              <>
                <Button
                  onClick={() => {
                    form.handleSubmit();
                  }}
                  disabled={!isFormValid || isCreating}
                  variant="outline"
                  className="px-4 py-2 border border-border-primary rounded-lg hover:bg-background-secondary transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                  data-testid="create-recipe-button"
                >
                  <Save className="w-4 h-4 mr-2" />
                  {isCreating ? intl.formatMessage(i18n.creating) : intl.formatMessage(i18n.createRecipe)}
                </Button>
                <Button
                  onClick={() => {
                    handleCreateRecipe(form.state.values, true);
                  }}
                  disabled={!isFormValid || isCreating}
                  className="px-4 py-2 text-text-inverse rounded-lg hover:bg-opacity-90 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                  data-testid="create-and-run-recipe-button"
                >
                  <Play className="w-4 h-4 mr-2" />
                  {isCreating ? intl.formatMessage(i18n.creating) : intl.formatMessage(i18n.createAndRunRecipe)}
                </Button>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
