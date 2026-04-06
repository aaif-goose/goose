import React, { useState, useEffect, useCallback } from 'react';
import { useForm } from '@tanstack/react-form';
import { Recipe, generateDeepLink, Parameter } from '../../recipe';
import { Check, ExternalLink, Play, Save, X } from 'lucide-react';
import { Geese } from '../icons/Geese';
import Copy from '../icons/Copy';
import { ExtensionConfig } from '../ConfigContext';
import { Button } from '../ui/button';
import type { Settings } from '../../api';

import { RecipeFormFields } from './shared/RecipeFormFields';
import { RecipeFormData } from './shared/recipeFormSchema';
import { toastSuccess, toastError } from '../../toasts';
import { saveRecipe } from '../../recipe/recipe_management';
import { errorMessage } from '../../utils/conversionUtils';
import { defineMessages, useIntl } from '../../i18n';

const i18n = defineMessages({
  createRecipeTitle: {
    id: 'createEditRecipe.createRecipeTitle',
    defaultMessage: 'Create Recipe',
  },
  viewEditRecipeTitle: {
    id: 'createEditRecipe.viewEditRecipeTitle',
    defaultMessage: 'View/edit recipe',
  },
  createSubtitle: {
    id: 'createEditRecipe.createSubtitle',
    defaultMessage: 'Create a new recipe to define agent behavior and capabilities for reusable chat sessions.',
  },
  editSubtitle: {
    id: 'createEditRecipe.editSubtitle',
    defaultMessage: "You can edit the recipe below to change the agent's behavior in a new session.",
  },
  learnMore: {
    id: 'createEditRecipe.learnMore',
    defaultMessage: 'Learn more',
  },
  copyLinkDescription: {
    id: 'createEditRecipe.copyLinkDescription',
    defaultMessage: 'Copy this link to share with friends or paste directly in Chrome to open',
  },
  copied: {
    id: 'createEditRecipe.copied',
    defaultMessage: 'Copied!',
  },
  copy: {
    id: 'createEditRecipe.copy',
    defaultMessage: 'Copy',
  },
  generatingDeeplink: {
    id: 'createEditRecipe.generatingDeeplink',
    defaultMessage: 'Generating deeplink...',
  },
  clickToGenerateDeeplink: {
    id: 'createEditRecipe.clickToGenerateDeeplink',
    defaultMessage: 'Click to generate deeplink',
  },
  close: {
    id: 'createEditRecipe.close',
    defaultMessage: 'Close',
  },
  saving: {
    id: 'createEditRecipe.saving',
    defaultMessage: 'Saving...',
  },
  saveRecipe: {
    id: 'createEditRecipe.saveRecipe',
    defaultMessage: 'Save Recipe',
  },
  saveAndRunRecipe: {
    id: 'createEditRecipe.saveAndRunRecipe',
    defaultMessage: 'Save & Run Recipe',
  },
  validationFailed: {
    id: 'createEditRecipe.validationFailed',
    defaultMessage: 'Validation Failed',
  },
  validationMsg: {
    id: 'createEditRecipe.validationMsg',
    defaultMessage: 'Please fill in all required fields and ensure JSON schema is valid.',
  },
  recipeSavedMsg: {
    id: 'createEditRecipe.recipeSavedMsg',
    defaultMessage: 'Recipe saved successfully',
  },
  saveFailed: {
    id: 'createEditRecipe.saveFailed',
    defaultMessage: 'Save Failed',
  },
  saveFailedMsg: {
    id: 'createEditRecipe.saveFailedMsg',
    defaultMessage: 'Failed to save recipe: {error}',
  },
  recipeSavedAndLaunchedMsg: {
    id: 'createEditRecipe.recipeSavedAndLaunchedMsg',
    defaultMessage: 'Recipe saved and launched successfully',
  },
  saveAndRunFailed: {
    id: 'createEditRecipe.saveAndRunFailed',
    defaultMessage: 'Save and Run Failed',
  },
  saveAndRunFailedMsg: {
    id: 'createEditRecipe.saveAndRunFailedMsg',
    defaultMessage: 'Failed to save and run recipe: {error}',
  },
});

interface CreateEditRecipeModalProps {
  isOpen: boolean;
  onClose: (wasSaved?: boolean) => void;
  recipe?: Recipe;
  isCreateMode?: boolean;
  recipeId?: string | null;
  onRecipeSaved?: (savedRecipeId: string) => void;
}

export default function CreateEditRecipeModal({
  isOpen,
  onClose,
  recipe,
  isCreateMode = false,
  recipeId,
  onRecipeSaved,
}: CreateEditRecipeModalProps) {
  const intl = useIntl();
  const getInitialValues = React.useCallback((): RecipeFormData => {
    if (recipe) {
      return {
        title: recipe.title || '',
        description: recipe.description || '',
        instructions: recipe.instructions || '',
        prompt: recipe.prompt || '',
        activities: recipe.activities || [],
        parameters: recipe.parameters || [],
        jsonSchema: recipe.response?.json_schema
          ? JSON.stringify(recipe.response.json_schema, null, 2)
          : '',
        model: recipe.settings?.goose_model ?? undefined,
        provider: recipe.settings?.goose_provider ?? undefined,
        extensions: recipe.extensions || undefined,
        subRecipes: (recipe.sub_recipes || []).map((sr) => ({
          name: sr.name,
          path: sr.path,
          description: sr.description || undefined,
          values: sr.values || undefined,
          sequential_when_repeated: sr.sequential_when_repeated ?? false,
        })),
      };
    }
    return {
      title: '',
      description: '',
      instructions: '',
      prompt: '',
      activities: [],
      parameters: [],
      jsonSchema: '',
      model: undefined,
      provider: undefined,
      extensions: undefined,
      subRecipes: [],
    };
  }, [recipe]);

  const form = useForm({
    defaultValues: getInitialValues(),
  });

  // Helper functions to get values from form - using state to trigger re-renders
  const [title, setTitle] = useState(form.state.values.title);
  const [description, setDescription] = useState(form.state.values.description);
  const [instructions, setInstructions] = useState(form.state.values.instructions);
  const [prompt, setPrompt] = useState(form.state.values.prompt);
  const [activities, setActivities] = useState(form.state.values.activities);
  const [parameters, setParameters] = useState(form.state.values.parameters);
  const [jsonSchema, setJsonSchema] = useState(form.state.values.jsonSchema);
  const [model, setModel] = useState(form.state.values.model);
  const [provider, setProvider] = useState(form.state.values.provider);
  const [extensions, setExtensions] = useState(form.state.values.extensions);
  const [subRecipes, setSubRecipes] = useState(form.state.values.subRecipes);

  // Subscribe to form changes to update local state
  useEffect(() => {
    return form.store.subscribe(() => {
      setTitle(form.state.values.title);
      setDescription(form.state.values.description);
      setInstructions(form.state.values.instructions);
      setPrompt(form.state.values.prompt);
      setActivities(form.state.values.activities);
      setParameters(form.state.values.parameters);
      setJsonSchema(form.state.values.jsonSchema);
      setModel(form.state.values.model);
      setProvider(form.state.values.provider);
      setExtensions(form.state.values.extensions);
      setSubRecipes(form.state.values.subRecipes);
    });
  }, [form]);
  const [copied, setCopied] = useState(false);
  const [isSaving, setIsSaving] = useState(false);

  // Reset form when recipe changes
  useEffect(() => {
    if (recipe) {
      const newValues = getInitialValues();
      form.reset(newValues);
    }
  }, [recipe, form, getInitialValues]);

  const getCurrentRecipe = useCallback((): Recipe => {
    // Transform the internal parameters state into the desired output format.
    const formattedParameters = parameters.map((param) => {
      const formattedParam: Parameter = {
        key: param.key,
        input_type: param.input_type || 'string',
        requirement: param.requirement,
        description: param.description,
      };

      // Add the 'default' key ONLY if the parameter is optional and has a default value.
      if (param.requirement === 'optional' && param.default) {
        formattedParam.default = param.default;
      }

      // Add options for select input type
      if (param.input_type === 'select' && param.options) {
        formattedParam.options = param.options.filter((opt) => opt.trim() !== ''); // Filter empty options when saving
      }

      return formattedParam;
    });

    // Parse response schema if provided
    let responseConfig = undefined;
    if (jsonSchema && jsonSchema.trim()) {
      try {
        const parsedSchema = JSON.parse(jsonSchema);
        responseConfig = { json_schema: parsedSchema };
      } catch (error) {
        console.warn('Invalid JSON schema provided:', error);
        // If JSON is invalid, don't include response config
      }
    }

    // Format subrecipes for API (convert from form data to API format)
    const formattedSubRecipes =
      subRecipes.length > 0
        ? subRecipes.map((subRecipe) => ({
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

    const cleanedExtensions = extensions?.map(
      (extension: ExtensionConfig & { envs?: unknown; enabled?: boolean }) => {
        const { envs: _envs, enabled: _enabled, ...rest } = extension;
        return rest;
      }
    ) as ExtensionConfig[] | undefined;

    const mergedSettings: Settings = {
      ...(recipe?.settings || {}),
    };
    if (model !== undefined) {
      mergedSettings.goose_model = model || null;
    } else if ('goose_model' in mergedSettings) {
      delete mergedSettings.goose_model;
    }
    if (provider !== undefined) {
      mergedSettings.goose_provider = provider || null;
    } else if ('goose_provider' in mergedSettings) {
      delete mergedSettings.goose_provider;
    }
    const settings = Object.values(mergedSettings).some(
      (value) => value !== undefined && value !== null
    )
      ? mergedSettings
      : undefined;

    return {
      ...recipe,
      title,
      description,
      instructions,
      activities,
      prompt: prompt || undefined,
      parameters: formattedParameters,
      response: responseConfig,
      sub_recipes: formattedSubRecipes,
      extensions: cleanedExtensions,
      settings,
    };
  }, [
    recipe,
    title,
    description,
    instructions,
    activities,
    prompt,
    parameters,
    jsonSchema,
    subRecipes,
    model,
    provider,
    extensions,
  ]);

  const requiredFieldsAreFilled = () => {
    return title.trim() && description.trim() && (instructions.trim() || (prompt || '').trim());
  };

  const validateForm = () => {
    const basicValidation =
      title.trim() && description.trim() && (instructions.trim() || (prompt || '').trim());

    // If JSON schema is provided, it must be valid
    if (jsonSchema && jsonSchema.trim()) {
      try {
        JSON.parse(jsonSchema);
      } catch {
        return false; // Invalid JSON schema fails validation
      }
    }

    return basicValidation;
  };

  const [deeplink, setDeeplink] = useState('');
  const [isGeneratingDeeplink, setIsGeneratingDeeplink] = useState(false);

  // Generate deeplink whenever recipe configuration changes
  useEffect(() => {
    let isCancelled = false;

    const generateLink = async () => {
      if (
        !title.trim() ||
        !description.trim() ||
        (!instructions.trim() && !(prompt || '').trim())
      ) {
        setDeeplink('');
        return;
      }

      setIsGeneratingDeeplink(true);
      try {
        const currentRecipe = getCurrentRecipe();
        const link = await generateDeepLink(currentRecipe);
        if (!isCancelled) {
          setDeeplink(link);
        }
      } catch (error) {
        console.error('Failed to generate deeplink:', error);
        if (!isCancelled) {
          setDeeplink('Error generating deeplink');
        }
      } finally {
        if (!isCancelled) {
          setIsGeneratingDeeplink(false);
        }
      }
    };

    generateLink();

    return () => {
      isCancelled = true;
    };
  }, [
    title,
    description,
    instructions,
    prompt,
    activities,
    parameters,
    jsonSchema,
    subRecipes,
    model,
    provider,
    extensions,
    getCurrentRecipe,
  ]);

  const handleCopy = () => {
    if (!deeplink || isGeneratingDeeplink || deeplink === 'Error generating deeplink') {
      return;
    }

    navigator.clipboard
      .writeText(deeplink)
      .then(() => {
        setCopied(true);
        setTimeout(() => setCopied(false), 2000);
      })
      .catch((err) => {
        console.error('Failed to copy the text:', err);
      });
  };

  const handleSaveRecipeClick = async () => {
    if (!validateForm()) {
      toastError({
        title: intl.formatMessage(i18n.validationFailed),
        msg: intl.formatMessage(i18n.validationMsg),
      });
      return;
    }

    setIsSaving(true);
    try {
      const recipe = getCurrentRecipe();

      const { id: savedRecipeId } = await saveRecipe(recipe, recipeId);

      if (onRecipeSaved) {
        onRecipeSaved(savedRecipeId);
      }

      onClose(true);

      toastSuccess({
        title: (recipe.title || '').trim(),
        msg: intl.formatMessage(i18n.recipeSavedMsg),
      });
    } catch (error) {
      console.error('Failed to save recipe:', error);

      toastError({
        title: intl.formatMessage(i18n.saveFailed),
        msg: intl.formatMessage(i18n.saveFailedMsg, { error: errorMessage(error, 'Unknown error') }),
        traceback: errorMessage(error),
      });
    } finally {
      setIsSaving(false);
    }
  };

  const handleSaveAndRunRecipeClick = async () => {
    if (!validateForm()) {
      toastError({
        title: intl.formatMessage(i18n.validationFailed),
        msg: intl.formatMessage(i18n.validationMsg),
      });
      return;
    }

    setIsSaving(true);
    try {
      const recipe = getCurrentRecipe();

      const { id: savedId } = await saveRecipe(recipe, recipeId);

      onClose(true);

      window.electron.createChatWindow({ recipeId: savedId });

      toastSuccess({
        title: recipe.title,
        msg: intl.formatMessage(i18n.recipeSavedAndLaunchedMsg),
      });
    } catch (error) {
      console.error('Failed to save and run recipe:', error);

      toastError({
        title: intl.formatMessage(i18n.saveAndRunFailed),
        msg: intl.formatMessage(i18n.saveAndRunFailedMsg, { error: errorMessage(error, 'Unknown error') }),
        traceback: errorMessage(error),
      });
    } finally {
      setIsSaving(false);
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-[400] flex items-center justify-center bg-black/50">
      <div className="bg-background-primary border border-border-primary rounded-lg w-[90vw] max-w-4xl h-[90vh] flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between p-6 border-b border-border-primary">
          <div className="flex items-center gap-3">
            <div className="w-8 h-8 bg-background-primary rounded-full flex items-center justify-center">
              <Geese className="w-6 h-6 text-iconProminent" />
            </div>
            <div>
              <h1 className="text-xl font-medium text-text-primary">
                {isCreateMode ? intl.formatMessage(i18n.createRecipeTitle) : intl.formatMessage(i18n.viewEditRecipeTitle)}
              </h1>
              <p className="text-text-secondary text-sm">
                {isCreateMode
                  ? intl.formatMessage(i18n.createSubtitle)
                  : intl.formatMessage(i18n.editSubtitle)}{' '}
                <a
                  href="https://block.github.io/goose/docs/guides/recipes/"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="inline-flex items-center gap-1 text-blue-500 hover:text-blue-600 hover:underline"
                >
                  {intl.formatMessage(i18n.learnMore)}
                  <ExternalLink className="w-3 h-3" />
                </a>
              </p>
            </div>
          </div>
          <Button
            onClick={() => onClose(false)}
            variant="ghost"
            size="sm"
            className="p-2 hover:bg-background-secondary rounded-lg transition-colors"
          >
            <X className="w-5 h-5" />
          </Button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto px-6 py-4">
          <RecipeFormFields form={form} />

          {/* Deep Link Display */}
          {requiredFieldsAreFilled() && (
            <div className="w-full p-4 bg-background-secondary rounded-lg mt-6">
              <div className="flex items-center justify-between mb-2">
                <div className="text-sm text-text-secondary">
                  {intl.formatMessage(i18n.copyLinkDescription)}
                </div>
                <Button
                  onClick={handleCopy}
                  variant="ghost"
                  size="sm"
                  disabled={
                    !deeplink || isGeneratingDeeplink || deeplink === 'Error generating deeplink'
                  }
                  className="ml-4 p-2 hover:bg-background-primary rounded-lg transition-colors flex items-center disabled:opacity-50 disabled:hover:bg-transparent"
                >
                  {copied ? (
                    <Check className="w-4 h-4 text-green-500" />
                  ) : (
                    <Copy className="w-4 h-4 text-iconSubtle" />
                  )}
                  <span className="ml-1 text-sm text-text-secondary">
                    {copied ? intl.formatMessage(i18n.copied) : intl.formatMessage(i18n.copy)}
                  </span>
                </Button>
              </div>
              <div
                onClick={handleCopy}
                className="text-sm truncate font-mono cursor-pointer text-text-primary"
              >
                {isGeneratingDeeplink
                  ? intl.formatMessage(i18n.generatingDeeplink)
                  : deeplink || intl.formatMessage(i18n.clickToGenerateDeeplink)}
              </div>
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex items-center justify-between p-6 border-t border-border-primary">
          <Button
            onClick={() => onClose(false)}
            variant="ghost"
            className="px-4 py-2 text-text-secondary rounded-lg hover:bg-background-secondary transition-colors"
          >
            {intl.formatMessage(i18n.close)}
          </Button>

          <div className="flex gap-3">
            <Button
              onClick={handleSaveRecipeClick}
              disabled={!requiredFieldsAreFilled() || isSaving}
              variant="outline"
              size="default"
              className="inline-flex items-center justify-center gap-2 px-4 py-2"
            >
              <Save className="w-4 h-4" />
              {isSaving ? intl.formatMessage(i18n.saving) : intl.formatMessage(i18n.saveRecipe)}
            </Button>
            <Button
              onClick={handleSaveAndRunRecipeClick}
              disabled={!requiredFieldsAreFilled() || isSaving}
              variant="default"
              size="default"
              className="inline-flex items-center justify-center gap-2 px-4 py-2"
            >
              <Play className="w-4 h-4" />
              {isSaving ? intl.formatMessage(i18n.saving) : intl.formatMessage(i18n.saveAndRunRecipe)}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
