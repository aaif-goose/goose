import { useState, useCallback } from 'react';
import { useForm } from '@tanstack/react-form';
import { X, Save, Loader2 } from 'lucide-react';
import { Button } from '../../ui/button';
import { toastSuccess, toastError } from '../../../toasts';
import { saveRecipe } from '../../../recipe/recipe_management';
import { Recipe } from '../../../recipe';
import { SubRecipeFormData } from './recipeFormSchema';
import { useEscapeKey } from '../../../hooks/useEscapeKey';
import KeyValueEditor from './KeyValueEditor';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  title: {
    id: 'createSubRecipeInline.title',
    defaultMessage: 'Create New Subrecipe',
  },
  subtitle: {
    id: 'createSubRecipeInline.subtitle',
    defaultMessage: 'Create a simple recipe to use as a callable tool in your main recipe',
  },
  closeModal: {
    id: 'createSubRecipeInline.closeModal',
    defaultMessage: 'Close create subrecipe modal',
  },
  nameLabel: {
    id: 'createSubRecipeInline.nameLabel',
    defaultMessage: 'Name',
  },
  namePlaceholder: {
    id: 'createSubRecipeInline.namePlaceholder',
    defaultMessage: 'e.g., security_scan',
  },
  nameHint: {
    id: 'createSubRecipeInline.nameHint',
    defaultMessage: 'Unique identifier used to generate the tool name',
  },
  recipeTitleLabel: {
    id: 'createSubRecipeInline.recipeTitleLabel',
    defaultMessage: 'Recipe Title',
  },
  recipeTitlePlaceholder: {
    id: 'createSubRecipeInline.recipeTitlePlaceholder',
    defaultMessage: 'e.g., Security Analysis Tool',
  },
  recipeDescriptionLabel: {
    id: 'createSubRecipeInline.recipeDescriptionLabel',
    defaultMessage: 'Recipe Description',
  },
  recipeDescriptionPlaceholder: {
    id: 'createSubRecipeInline.recipeDescriptionPlaceholder',
    defaultMessage: 'What this recipe does when executed',
  },
  instructionsLabel: {
    id: 'createSubRecipeInline.instructionsLabel',
    defaultMessage: 'Instructions',
  },
  instructionsPlaceholder: {
    id: 'createSubRecipeInline.instructionsPlaceholder',
    defaultMessage: 'Instructions for the AI when this subrecipe is called...',
  },
  toolDescriptionLabel: {
    id: 'createSubRecipeInline.toolDescriptionLabel',
    defaultMessage: 'Tool Description',
  },
  toolDescriptionPlaceholder: {
    id: 'createSubRecipeInline.toolDescriptionPlaceholder',
    defaultMessage: 'Optional description shown when this is called as a tool',
  },
  sequentialLabel: {
    id: 'createSubRecipeInline.sequentialLabel',
    defaultMessage: 'Sequential when repeated',
  },
  sequentialHint: {
    id: 'createSubRecipeInline.sequentialHint',
    defaultMessage: '(Forces sequential execution of multiple instances)',
  },
  preconfiguredValues: {
    id: 'createSubRecipeInline.preconfiguredValues',
    defaultMessage: 'Pre-configured Values',
  },
  preconfiguredValuesHint: {
    id: 'createSubRecipeInline.preconfiguredValuesHint',
    defaultMessage: 'Optional parameter values that are always passed to the subrecipe',
  },
  cancel: {
    id: 'createSubRecipeInline.cancel',
    defaultMessage: 'Cancel',
  },
  creating: {
    id: 'createSubRecipeInline.creating',
    defaultMessage: 'Creating...',
  },
  createAndAdd: {
    id: 'createSubRecipeInline.createAndAdd',
    defaultMessage: 'Create & Add Subrecipe',
  },
  validationFailed: {
    id: 'createSubRecipeInline.validationFailed',
    defaultMessage: 'Validation Failed',
  },
  validationMsg: {
    id: 'createSubRecipeInline.validationMsg',
    defaultMessage: 'Name, title, recipe description, and instructions are required.',
  },
  duplicateName: {
    id: 'createSubRecipeInline.duplicateName',
    defaultMessage: 'Duplicate Name',
  },
  duplicateNameMsg: {
    id: 'createSubRecipeInline.duplicateNameMsg',
    defaultMessage: 'A subrecipe named "{name}" already exists. Please use a unique name.',
  },
  createdSuccess: {
    id: 'createSubRecipeInline.createdSuccess',
    defaultMessage: 'Subrecipe created successfully',
  },
  saveFailed: {
    id: 'createSubRecipeInline.saveFailed',
    defaultMessage: 'Save Failed',
  },
  saveFailedMsg: {
    id: 'createSubRecipeInline.saveFailedMsg',
    defaultMessage: 'Failed to save subrecipe: {error}',
  },
});

interface CreateSubRecipeInlineProps {
  isOpen: boolean;
  onClose: () => void;
  onSubRecipeSaved: (subRecipe: SubRecipeFormData) => void;
  existingSubRecipes?: SubRecipeFormData[];
}

export default function CreateSubRecipeInline({
  isOpen,
  onClose,
  onSubRecipeSaved,
  existingSubRecipes = [],
}: CreateSubRecipeInlineProps) {
  const intl = useIntl();
  useEscapeKey(isOpen, onClose);

  const form = useForm({
    defaultValues: {
      title: '',
      description: '',
      instructions: '',
      prompt: '',
      activities: [],
      parameters: [],
      jsonSchema: '',
      subRecipes: [],
    },
  });

  const [name, setName] = useState('');
  const [toolDescription, setToolDescription] = useState('');
  const [sequentialWhenRepeated, setSequentialWhenRepeated] = useState(false);
  const [values, setValues] = useState<Record<string, string>>({});
  const [isSaving, setIsSaving] = useState(false);

  const handleSave = useCallback(async () => {
    const formValues = form.state.values;

    if (
      !name.trim() ||
      !formValues.title.trim() ||
      !formValues.description.trim() ||
      !formValues.instructions.trim()
    ) {
      toastError({
        title: intl.formatMessage(i18n.validationFailed),
        msg: intl.formatMessage(i18n.validationMsg),
      });
      return;
    }

    const trimmedName = name.trim();
    if (existingSubRecipes.some((sr) => sr.name === trimmedName)) {
      toastError({
        title: intl.formatMessage(i18n.duplicateName),
        msg: intl.formatMessage(i18n.duplicateNameMsg, { name: trimmedName }),
      });
      return;
    }

    setIsSaving(true);
    try {
      const recipe: Recipe = {
        version: '1.0.0',
        title: formValues.title.trim(),
        description: formValues.description.trim(),
        instructions: formValues.instructions.trim(),
      };

      const { filePath } = await saveRecipe(recipe, null);

      const subRecipe: SubRecipeFormData = {
        name: trimmedName,
        path: filePath,
        description: toolDescription.trim() || undefined,
        sequential_when_repeated: sequentialWhenRepeated,
        values: Object.keys(values).length > 0 ? values : undefined,
      };

      toastSuccess({
        title: formValues.title.trim(),
        msg: intl.formatMessage(i18n.createdSuccess),
      });

      onSubRecipeSaved(subRecipe);
      onClose();
      form.reset();
      setName('');
      setToolDescription('');
      setSequentialWhenRepeated(false);
      setValues({});
    } catch (error) {
      console.error('Failed to save subrecipe:', error);

      toastError({
        title: intl.formatMessage(i18n.saveFailed),
        msg: intl.formatMessage(i18n.saveFailedMsg, { error: error instanceof Error ? error.message : 'Unknown error' }),
      });
    } finally {
      setIsSaving(false);
    }
  }, [form, name, toolDescription, sequentialWhenRepeated, values, existingSubRecipes, onSubRecipeSaved, onClose, intl]);

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-[500] flex items-center justify-center bg-black/50">
      <div className="bg-background-primary border border-borderSubtle rounded-lg w-[90vw] max-w-2xl max-h-[90vh] flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between p-6 border-b border-borderSubtle">
          <div>
            <h2 className="text-xl font-medium text-textProminent">{intl.formatMessage(i18n.title)}</h2>
            <p className="text-textSubtle text-sm">
              {intl.formatMessage(i18n.subtitle)}
            </p>
          </div>
          <Button
            onClick={onClose}
            variant="ghost"
            size="sm"
            className="p-2 hover:bg-bgSubtle rounded-lg transition-colors"
            aria-label={intl.formatMessage(i18n.closeModal)}
          >
            <X className="w-5 h-5" />
          </Button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto px-6 py-4 space-y-4">
          {/* Name Field */}
          <div>
            <label
              htmlFor="subrecipe-name"
              className="block text-sm font-medium text-text-standard mb-2"
            >
              {intl.formatMessage(i18n.nameLabel)} <span className="text-text-danger">*</span>
            </label>
            <input
              id="subrecipe-name"
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="w-full p-3 border border-border-subtle rounded-lg bg-background-primary text-text-standard focus:outline-none focus:ring-2 focus:ring-ring"
              placeholder={intl.formatMessage(i18n.namePlaceholder)}
            />
            <p className="text-xs text-text-muted mt-1">
              {intl.formatMessage(i18n.nameHint)}
            </p>
          </div>

          {/* Title Field */}
          <form.Field name="title">
            {(field) => (
              <div>
                <label
                  htmlFor="subrecipe-title"
                  className="block text-sm font-medium text-text-standard mb-2"
                >
                  {intl.formatMessage(i18n.recipeTitleLabel)} <span className="text-text-danger">*</span>
                </label>
                <input
                  id="subrecipe-title"
                  type="text"
                  value={field.state.value}
                  onChange={(e) => field.handleChange(e.target.value)}
                  onBlur={field.handleBlur}
                  className="w-full p-3 border border-border-subtle rounded-lg bg-background-primary text-text-standard focus:outline-none focus:ring-2 focus:ring-ring"
                  placeholder={intl.formatMessage(i18n.recipeTitlePlaceholder)}
                />
              </div>
            )}
          </form.Field>

          {/* Recipe Description Field */}
          <form.Field name="description">
            {(field) => (
              <div>
                <label
                  htmlFor="recipe-description"
                  className="block text-sm font-medium text-text-standard mb-2"
                >
                  {intl.formatMessage(i18n.recipeDescriptionLabel)} <span className="text-text-danger">*</span>
                </label>
                <input
                  id="recipe-description"
                  type="text"
                  value={field.state.value}
                  onChange={(e) => field.handleChange(e.target.value)}
                  onBlur={field.handleBlur}
                  className="w-full p-3 border border-border-subtle rounded-lg bg-background-primary text-text-standard focus:outline-none focus:ring-2 focus:ring-ring"
                  placeholder={intl.formatMessage(i18n.recipeDescriptionPlaceholder)}
                />
              </div>
            )}
          </form.Field>

          {/* Instructions Field */}
          <form.Field name="instructions">
            {(field) => (
              <div>
                <label
                  htmlFor="subrecipe-instructions"
                  className="block text-sm font-medium text-text-standard mb-2"
                >
                  {intl.formatMessage(i18n.instructionsLabel)} <span className="text-text-danger">*</span>
                </label>
                <textarea
                  id="subrecipe-instructions"
                  value={field.state.value}
                  onChange={(e) => field.handleChange(e.target.value)}
                  onBlur={field.handleBlur}
                  className="w-full p-3 border border-border-subtle rounded-lg bg-background-primary text-text-standard focus:outline-none focus:ring-2 focus:ring-ring resize-none font-mono text-sm"
                  placeholder={intl.formatMessage(i18n.instructionsPlaceholder)}
                  rows={8}
                />
              </div>
            )}
          </form.Field>

          {/* Tool Description Field */}
          <div>
            <label
              htmlFor="tool-description"
              className="block text-sm font-medium text-text-standard mb-2"
            >
              {intl.formatMessage(i18n.toolDescriptionLabel)}
            </label>
            <textarea
              id="tool-description"
              value={toolDescription}
              onChange={(e) => setToolDescription(e.target.value)}
              className="w-full p-3 border border-border-subtle rounded-lg bg-background-primary text-text-standard focus:outline-none focus:ring-2 focus:ring-ring resize-none"
              placeholder={intl.formatMessage(i18n.toolDescriptionPlaceholder)}
              rows={2}
            />
          </div>

          {/* Sequential When Repeated */}
          <div className="flex items-center gap-2">
            <input
              id="subrecipe-sequential"
              type="checkbox"
              checked={sequentialWhenRepeated}
              onChange={(e) => setSequentialWhenRepeated(e.target.checked)}
              className="w-4 h-4 border-border-subtle rounded focus:ring-2 focus:ring-ring"
            />
            <label htmlFor="subrecipe-sequential" className="text-sm text-text-standard">
              {intl.formatMessage(i18n.sequentialLabel)}
            </label>
            <span className="text-xs text-text-muted">
              {intl.formatMessage(i18n.sequentialHint)}
            </span>
          </div>

          {/* Pre-configured Values */}
          <div>
            <label className="block text-sm font-medium text-text-standard mb-2">
              {intl.formatMessage(i18n.preconfiguredValues)}
            </label>
            <p className="text-xs text-text-muted mb-3">
              {intl.formatMessage(i18n.preconfiguredValuesHint)}
            </p>
            <KeyValueEditor values={values} onChange={setValues} />
          </div>
        </div>

        {/* Footer */}
        <div className="flex gap-3 p-6 border-t border-borderSubtle justify-end">
          <Button onClick={onClose} variant="outline">
            {intl.formatMessage(i18n.cancel)}
          </Button>
          <Button
            onClick={handleSave}
            disabled={
              !name.trim() ||
              !form.state.values.title.trim() ||
              !form.state.values.description.trim() ||
              !form.state.values.instructions.trim() ||
              isSaving
            }
            className="inline-flex items-center gap-2"
          >
            {isSaving ? (
              <>
                <Loader2 className="w-4 h-4 animate-spin" />
                {intl.formatMessage(i18n.creating)}
              </>
            ) : (
              <>
                <Save className="w-4 h-4" />
                {intl.formatMessage(i18n.createAndAdd)}
              </>
            )}
          </Button>
        </div>
      </div>
    </div>
  );
}
