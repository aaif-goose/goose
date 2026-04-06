import React, { useState } from 'react';
import { Parameter } from '../../../recipe';
import { ChevronDown } from 'lucide-react';
import { ExtensionConfig } from '../../../api';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  titleLabel: {
    id: 'recipeFormFields.titleLabel',
    defaultMessage: 'Title',
  },
  titlePlaceholder: {
    id: 'recipeFormFields.titlePlaceholder',
    defaultMessage: 'Recipe title',
  },
  descriptionLabel: {
    id: 'recipeFormFields.descriptionLabel',
    defaultMessage: 'Description',
  },
  descriptionPlaceholder: {
    id: 'recipeFormFields.descriptionPlaceholder',
    defaultMessage: 'Brief description of what this recipe does',
  },
  instructionsLabel: {
    id: 'recipeFormFields.instructionsLabel',
    defaultMessage: 'Instructions',
  },
  openEditor: {
    id: 'recipeFormFields.openEditor',
    defaultMessage: 'Open Editor',
  },
  instructionsPlaceholder: {
    id: 'recipeFormFields.instructionsPlaceholder',
    defaultMessage: 'Detailed instructions for the AI, hidden from the user',
  },
  templateVarHint: {
    id: 'recipeFormFields.templateVarHint',
    defaultMessage: "Use '{{parameter_name}}' to define parameters that can be filled in when running the recipe.",
  },
  initialPrompt: {
    id: 'recipeFormFields.initialPrompt',
    defaultMessage: 'Initial Prompt',
  },
  promptOptionalHint: {
    id: 'recipeFormFields.promptOptionalHint',
    defaultMessage: '(Optional - Instructions or Prompt are required)',
  },
  promptPlaceholder: {
    id: 'recipeFormFields.promptPlaceholder',
    defaultMessage: 'Pre-filled prompt when the recipe starts',
  },
  advancedOptions: {
    id: 'recipeFormFields.advancedOptions',
    defaultMessage: 'Advanced Options',
  },
  advancedOptionsHint: {
    id: 'recipeFormFields.advancedOptionsHint',
    defaultMessage: 'Activities, parameters, model, extensions, response schema, subrecipes',
  },
  parametersLabel: {
    id: 'recipeFormFields.parametersLabel',
    defaultMessage: 'Parameters',
  },
  parametersDescription: {
    id: 'recipeFormFields.parametersDescription',
    defaultMessage: "Parameters will be automatically detected from '{{parameter_name}}' syntax in instructions/prompt/activities or you can manually add them below.",
  },
  parameterNamePlaceholder: {
    id: 'recipeFormFields.parameterNamePlaceholder',
    defaultMessage: 'Enter parameter name...',
  },
  addParameter: {
    id: 'recipeFormFields.addParameter',
    defaultMessage: 'Add parameter',
  },
  enterValueFor: {
    id: 'recipeFormFields.enterValueFor',
    defaultMessage: 'Enter value for {key}',
  },
  responseJsonSchema: {
    id: 'recipeFormFields.responseJsonSchema',
    defaultMessage: 'Response JSON Schema',
  },
  responseJsonSchemaDescription: {
    id: 'recipeFormFields.responseJsonSchemaDescription',
    defaultMessage: "Define the expected structure of the AI's response using JSON Schema format",
  },
});

import ParameterInput from '../../parameter/ParameterInput';
import RecipeActivityEditor from '../RecipeActivityEditor';
import JsonSchemaEditor from './JsonSchemaEditor';
import InstructionsEditor from './InstructionsEditor';
import SubRecipeEditor from './SubRecipeEditor';
import { Button } from '../../ui/button';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '../../ui/collapsible';
import { RecipeFormApi, RecipeFormData, SubRecipeFormData } from './recipeFormSchema';
import { RecipeModelSelector } from './RecipeModelSelector';
import { RecipeExtensionSelector } from './RecipeExtensionSelector';

// Type for field API to avoid linting issues - use any to bypass complex type constraints
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type FormFieldApi<_T = any> = any;

interface RecipeFormFieldsProps {
  // Form instance from parent
  form: RecipeFormApi;

  // Event handlers
  onTitleChange?: (value: string) => void;
  onDescriptionChange?: (value: string) => void;
  onInstructionsChange?: (value: string) => void;
  onPromptChange?: (value: string) => void;
  onJsonSchemaChange?: (value: string) => void;
}

export const extractTemplateVariables = (content: string): string[] => {
  const templateVarRegex = /\{\{(.*?)\}\}/g;
  const variables: string[] = [];
  let match;

  while ((match = templateVarRegex.exec(content)) !== null) {
    const variable = match[1].trim();

    if (variable && !variables.includes(variable)) {
      // Filter out complex variables that aren't valid parameter names
      // This matches the backend logic in filter_complex_variables()
      const validVarRegex = /^\s*[a-zA-Z_][a-zA-Z0-9_]*\s*$/;
      if (validVarRegex.test(variable)) {
        variables.push(variable);
      }
    }
  }

  return variables;
};

export function RecipeFormFields({
  form,
  onTitleChange,
  onDescriptionChange,
  onInstructionsChange,
  onPromptChange,
  onJsonSchemaChange,
}: RecipeFormFieldsProps) {
  const intl = useIntl();
  const [showJsonSchemaEditor, setShowJsonSchemaEditor] = useState(false);
  const [showInstructionsEditor, setShowInstructionsEditor] = useState(false);
  const [newParameterName, setNewParameterName] = useState('');
  const [expandedParameters, setExpandedParameters] = useState<Set<string>>(new Set());

  // Force re-render when instructions, prompt, or activities change
  const [_forceRender, setForceRender] = useState(0);

  React.useEffect(() => {
    return form.store.subscribe(() => {
      // Force re-render when any form field changes to update parameter usage indicators
      setForceRender((prev) => prev + 1);
    });
  }, [form.store]);

  const parseParametersFromInstructions = React.useCallback(
    (instructions: string, prompt?: string, activities?: string[]): Parameter[] => {
      const instructionVars = extractTemplateVariables(instructions);
      const promptVars = prompt ? extractTemplateVariables(prompt) : [];
      const activityVars = activities
        ? activities.flatMap((activity) => extractTemplateVariables(activity))
        : [];

      // Combine and deduplicate
      const allVars = [...new Set([...instructionVars, ...promptVars, ...activityVars])];

      return allVars.map((key: string) => ({
        key,
        description: intl.formatMessage(i18n.enterValueFor, { key }),
        requirement: 'required' as const,
        input_type: 'string' as const,
      }));
    },
    [intl]
  );

  // Function to update parameters based on current field values
  const updateParametersFromFields = React.useCallback(() => {
    const currentValues = form.state.values;
    const { instructions, prompt, activities, parameters: currentParams } = currentValues;

    const newParams = parseParametersFromInstructions(instructions, prompt, activities);

    // Separate manually added parameters (those not found in instructions/prompt/activities)
    const manualParams = currentParams.filter((param: Parameter) => {
      // Only keep manual params that have a valid key and are not found in the parsed params
      return (
        param.key && param.key.trim() && !newParams.some((newParam) => newParam.key === param.key)
      );
    });

    // Combine parsed parameters with manually added ones, filtering out empty ones
    const combinedParams = [
      ...newParams.map((newParam) => {
        const existing = currentParams.find((cp: Parameter) => cp.key === newParam.key);
        return existing ? { ...existing } : newParam;
      }),
      ...manualParams,
    ].filter((param: Parameter) => param.key && param.key.trim()) as Parameter[];

    // Only update if parameters actually changed
    const currentParamKeys = currentParams.map((p: Parameter) => p.key).sort();
    const newParamKeys = combinedParams.map((p) => p.key).sort();

    if (JSON.stringify(currentParamKeys) !== JSON.stringify(newParamKeys)) {
      form.setFieldValue('parameters', combinedParams);
    }
  }, [form, parseParametersFromInstructions]);

  const isParameterUsed = (
    paramKey: string,
    instructions: string,
    prompt?: string,
    activities?: string[]
  ): boolean => {
    const regex = new RegExp(
      `\\{\\{\\s*${paramKey.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*\\}\\}`,
      'g'
    );
    const usedInInstructions = regex.test(instructions);
    const usedInPrompt = prompt ? regex.test(prompt) : false;
    const usedInActivities = activities
      ? activities.some((activity) => {
          // For activities, we need to check the full activity string, including message: prefixes
          return regex.test(activity);
        })
      : false;
    return usedInInstructions || usedInPrompt || usedInActivities;
  };

  const checkHasAdvancedData = React.useCallback((values: RecipeFormData) => {
    const hasActivities = Boolean(values.activities && values.activities.length > 0);
    const hasParameters = Boolean(values.parameters && values.parameters.length > 0);
    const hasJsonSchema = Boolean(values.jsonSchema && values.jsonSchema.trim());
    const hasModel = Boolean(values.model && values.model.trim());
    const hasProvider = Boolean(values.provider && values.provider.trim());
    const hasExtensions = Boolean(values.extensions && values.extensions.length > 0);
    return (
      hasActivities || hasParameters || hasJsonSchema || hasModel || hasProvider || hasExtensions
    );
  }, []);

  const [advancedOpen, setAdvancedOpen] = useState(() => checkHasAdvancedData(form.state.values));

  return (
    <div className="space-y-4" data-testid="recipe-form">
      {/* Title Field */}
      <form.Field name="title">
        {(field: FormFieldApi<string>) => (
          <div>
            <label
              htmlFor="recipe-title"
              className="block text-sm font-medium text-text-primary mb-2"
            >
              {intl.formatMessage(i18n.titleLabel)} <span className="text-red-500">*</span>
            </label>
            <input
              id="recipe-title"
              type="text"
              value={field.state.value}
              onChange={(e) => {
                field.handleChange(e.target.value);
                onTitleChange?.(e.target.value);
              }}
              onBlur={field.handleBlur}
              className={`w-full p-3 border rounded-lg bg-background-primary text-text-primary focus:outline-none focus:ring-2 focus:ring-blue-500 ${
                field.state.meta.errors.length > 0 ? 'border-red-500' : 'border-border-primary'
              }`}
              placeholder={intl.formatMessage(i18n.titlePlaceholder)}
              data-testid="title-input"
            />
            {field.state.meta.errors.length > 0 && (
              <p className="text-red-500 text-sm mt-1">{field.state.meta.errors[0]}</p>
            )}
          </div>
        )}
      </form.Field>

      {/* Description Field */}
      <form.Field name="description">
        {(field: FormFieldApi<string>) => (
          <div>
            <label
              htmlFor="recipe-description"
              className="block text-sm font-medium text-text-primary mb-2"
            >
              {intl.formatMessage(i18n.descriptionLabel)} <span className="text-red-500">*</span>
            </label>
            <input
              id="recipe-description"
              type="text"
              value={field.state.value}
              onChange={(e) => {
                field.handleChange(e.target.value);
                onDescriptionChange?.(e.target.value);
              }}
              onBlur={field.handleBlur}
              className={`w-full p-3 border rounded-lg bg-background-primary text-text-primary focus:outline-none focus:ring-2 focus:ring-blue-500 ${
                field.state.meta.errors.length > 0 ? 'border-red-500' : 'border-border-primary'
              }`}
              placeholder={intl.formatMessage(i18n.descriptionPlaceholder)}
              data-testid="description-input"
            />
            {field.state.meta.errors.length > 0 && (
              <p className="text-red-500 text-sm mt-1">{field.state.meta.errors[0]}</p>
            )}
          </div>
        )}
      </form.Field>

      {/* Instructions Field */}
      <form.Field name="instructions">
        {(field: FormFieldApi<string>) => (
          <div>
            <div className="flex items-center justify-between mb-2">
              <label
                htmlFor="recipe-instructions"
                className="block text-sm font-medium text-text-primary"
              >
                {intl.formatMessage(i18n.instructionsLabel)} <span className="text-red-500">*</span>
              </label>
              <Button
                type="button"
                onClick={() => setShowInstructionsEditor(true)}
                variant="outline"
                size="sm"
                className="text-xs"
              >
                {intl.formatMessage(i18n.openEditor)}
              </Button>
            </div>
            <textarea
              id="recipe-instructions"
              value={field.state.value}
              onChange={(e) => {
                field.handleChange(e.target.value);
                onInstructionsChange?.(e.target.value);
              }}
              onBlur={() => {
                field.handleBlur();
                updateParametersFromFields();
              }}
              className={`w-full p-3 border rounded-lg bg-background-primary text-text-primary focus:outline-none focus:ring-2 focus:ring-blue-500 resize-none font-mono text-sm ${
                field.state.meta.errors.length > 0 ? 'border-red-500' : 'border-border-primary'
              }`}
              placeholder={intl.formatMessage(i18n.instructionsPlaceholder)}
              rows={8}
              data-testid="instructions-input"
            />
            <p className="text-xs text-text-secondary mt-1">
              {intl.formatMessage(i18n.templateVarHint)}
            </p>
            {field.state.meta.errors.length > 0 && (
              <p className="text-red-500 text-sm mt-1">{field.state.meta.errors[0]}</p>
            )}

            {/* Instructions Editor Modal */}
            <InstructionsEditor
              isOpen={showInstructionsEditor}
              onClose={() => setShowInstructionsEditor(false)}
              value={field.state.value}
              onChange={(value) => {
                field.handleChange(value);
                onInstructionsChange?.(value);
                updateParametersFromFields();
              }}
              error={field.state.meta.errors.length > 0 ? field.state.meta.errors[0] : undefined}
            />
          </div>
        )}
      </form.Field>

      {/* Initial Prompt Field */}
      <form.Field name="prompt">
        {(field: FormFieldApi<string | undefined>) => (
          <div>
            <label
              htmlFor="recipe-prompt"
              className="block text-sm font-medium text-text-primary mb-2"
            >
              {intl.formatMessage(i18n.initialPrompt)}
            </label>
            <p className="text-xs text-text-secondary mt-2 mb-2">
              {intl.formatMessage(i18n.promptOptionalHint)}
            </p>
            <textarea
              id="recipe-prompt"
              value={field.state.value || ''}
              onChange={(e) => {
                field.handleChange(e.target.value);
                onPromptChange?.(e.target.value);
              }}
              onBlur={() => {
                field.handleBlur();
                updateParametersFromFields();
              }}
              className="w-full p-3 border border-border-primary rounded-lg bg-background-primary text-text-primary focus:outline-none focus:ring-2 focus:ring-blue-500 resize-none"
              placeholder={intl.formatMessage(i18n.promptPlaceholder)}
              rows={3}
              data-testid="prompt-input"
            />
          </div>
        )}
      </form.Field>

      {/* Advanced Section - Collapsible */}
      <Collapsible open={advancedOpen} onOpenChange={setAdvancedOpen} className="mt-6">
        <CollapsibleTrigger className="flex items-baseline gap-2 w-full py-3 px-4 bg-background-secondary hover:bg-background-secondary/80 rounded-lg transition-colors border border-border-primary">
          <ChevronDown
            className={`w-4 h-4 text-text-secondary transition-transform duration-200 flex-shrink-0 relative top-0.5 ${
              advancedOpen ? 'rotate-0' : '-rotate-90'
            }`}
          />
          <span className="text-sm font-medium text-textStandard">{intl.formatMessage(i18n.advancedOptions)}</span>
          <span className="text-xs text-textSubtle">
            {intl.formatMessage(i18n.advancedOptionsHint)}
          </span>
        </CollapsibleTrigger>

        <CollapsibleContent className="mt-4 space-y-4 pl-6 border-l-2 border-border-primary ml-2">
          {/* Activities Field */}
          <form.Field name="activities">
            {(field: FormFieldApi<string[]>) => (
              <div>
                <RecipeActivityEditor
                  activities={field.state.value}
                  setActivities={(activities) => field.handleChange(activities)}
                  onBlur={updateParametersFromFields}
                />
              </div>
            )}
          </form.Field>

          {/* Parameters Field */}
          <form.Field name="parameters">
            {(field: FormFieldApi<Parameter[]>) => {
              const handleAddParameter = () => {
                if (newParameterName.trim()) {
                  const newParam: Parameter = {
                    key: newParameterName.trim(),
                    description: intl.formatMessage(i18n.enterValueFor, { key: newParameterName.trim() }),
                    input_type: 'string',
                    requirement: 'required',
                  };
                  field.handleChange([...field.state.value, newParam]);
                  setNewParameterName('');
                  // Expand the newly added parameter by default
                  setExpandedParameters((prev) => {
                    const newSet = new Set(prev);
                    newSet.add(newParam.key);
                    return newSet;
                  });
                }
              };

              const handleKeyDown = (e: React.KeyboardEvent) => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  handleAddParameter();
                }
              };

              const handleDeleteParameter = (parameterKey: string) => {
                const updatedParams = field.state.value.filter(
                  (param: Parameter) => param.key !== parameterKey
                );
                field.handleChange(updatedParams);
                // Remove from expanded set if it was expanded
                setExpandedParameters((prev) => {
                  const newSet = new Set(prev);
                  newSet.delete(parameterKey);
                  return newSet;
                });
              };

              const handleToggleExpanded = (parameterKey: string) => {
                setExpandedParameters((prev) => {
                  const newSet = new Set(prev);
                  if (newSet.has(parameterKey)) {
                    newSet.delete(parameterKey);
                  } else {
                    newSet.add(parameterKey);
                  }
                  return newSet;
                });
              };

              return (
                <div>
                  <label className="block text-md text-text-primary mb-2 font-bold">
                    {intl.formatMessage(i18n.parametersLabel)}
                  </label>
                  <p className="text-text-secondary text-sm space-y-2 pb-4">
                    {intl.formatMessage(i18n.parametersDescription)}
                  </p>

                  {/* Add Parameter Input - Always Visible */}
                  <div className="flex gap-2 mb-4">
                    <input
                      type="text"
                      value={newParameterName}
                      onChange={(e) => setNewParameterName(e.target.value)}
                      onKeyDown={handleKeyDown}
                      placeholder={intl.formatMessage(i18n.parameterNamePlaceholder)}
                      className="flex-1 px-3 py-2 border border-border-primary rounded-lg bg-background-primary text-text-primary focus:outline-none focus:ring-2 focus:ring-blue-500 text-sm"
                    />
                    <button
                      type="button"
                      onClick={handleAddParameter}
                      disabled={!newParameterName.trim()}
                      className="px-4 py-2 bg-blue-500 text-white rounded-lg text-sm hover:bg-blue-600 transition-colors disabled:bg-gray-400 disabled:cursor-not-allowed"
                    >
                      {intl.formatMessage(i18n.addParameter)}
                    </button>
                  </div>

                  {field.state.value.length > 0 &&
                    field.state.value
                      .filter((parameter: Parameter) => parameter.key && parameter.key.trim()) // Filter out empty parameters
                      .map((parameter: Parameter) => {
                        const currentValues = form.state.values;
                        const isUnused = !isParameterUsed(
                          parameter.key,
                          currentValues.instructions,
                          currentValues.prompt,
                          currentValues.activities
                        );

                        return (
                          <ParameterInput
                            key={parameter.key}
                            parameter={parameter}
                            isUnused={isUnused}
                            isExpanded={expandedParameters.has(parameter.key)}
                            onToggleExpanded={handleToggleExpanded}
                            onDelete={handleDeleteParameter}
                            onChange={(name, value) => {
                              const updatedParams = field.state.value.map((param: Parameter) =>
                                param.key === name ? { ...param, ...value } : param
                              );
                              field.handleChange(updatedParams);
                            }}
                          />
                        );
                      })}
                </div>
              );
            }}
          </form.Field>

          {/* Model and Provider Fields */}
          <form.Field name="provider">
            {(providerField: FormFieldApi<string | undefined>) => (
              <form.Field name="model">
                {(modelField: FormFieldApi<string | undefined>) => (
                  <RecipeModelSelector
                    selectedProvider={providerField.state.value}
                    selectedModel={modelField.state.value}
                    onProviderChange={(provider) => providerField.handleChange(provider)}
                    onModelChange={(model) => modelField.handleChange(model)}
                  />
                )}
              </form.Field>
            )}
          </form.Field>

          {/* Extensions Field */}
          <form.Field name="extensions">
            {(field: FormFieldApi<ExtensionConfig[] | undefined>) => (
              <RecipeExtensionSelector
                selectedExtensions={field.state.value || []}
                onExtensionsChange={(extensions) =>
                  field.handleChange(extensions.length > 0 ? extensions : undefined)
                }
              />
            )}
          </form.Field>

          {/* JSON Schema Field */}
          <form.Field name="jsonSchema">
            {(field: FormFieldApi<string | undefined>) => (
              <div>
                <label className="block text-md text-text-primary mb-2 font-bold">
                  {intl.formatMessage(i18n.responseJsonSchema)}
                </label>
                <p className="text-text-secondary text-sm space-y-2 pb-4">
                  {intl.formatMessage(i18n.responseJsonSchemaDescription)}
                </p>
                <div className="flex items-center justify-between mb-2">
                  <Button
                    type="button"
                    onClick={() => setShowJsonSchemaEditor(true)}
                    variant="outline"
                    size="sm"
                    className="text-xs"
                  >
                    {intl.formatMessage(i18n.openEditor)}
                  </Button>
                </div>

                {field.state.value && field.state.value.trim() && (
                  <div
                    className={`border rounded-lg p-3 bg-background-secondary ${
                      field.state.meta.errors.length > 0
                        ? 'border-red-500'
                        : 'border-border-primary'
                    }`}
                  >
                    <pre className="text-xs font-mono text-text-primary whitespace-pre-wrap break-words max-h-32 overflow-y-auto">
                      {field.state.value}
                    </pre>
                  </div>
                )}

                {field.state.meta.errors.length > 0 && (
                  <p className="text-red-500 text-sm mt-1">{field.state.meta.errors[0]}</p>
                )}

                {/* JSON Schema Editor Modal */}
                <JsonSchemaEditor
                  isOpen={showJsonSchemaEditor}
                  onClose={() => setShowJsonSchemaEditor(false)}
                  value={field.state.value || ''}
                  onChange={(value) => {
                    field.handleChange(value);
                    onJsonSchemaChange?.(value);
                  }}
                  error={
                    field.state.meta.errors.length > 0 ? field.state.meta.errors[0] : undefined
                  }
                />
              </div>
            )}
          </form.Field>

          {/* Subrecipes Field */}
          <form.Field name="subRecipes">
            {(field: FormFieldApi<SubRecipeFormData[]>) => (
              <div>
                <SubRecipeEditor
                  subRecipes={field.state.value}
                  onChange={(subRecipes) => field.handleChange(subRecipes)}
                />
              </div>
            )}
          </form.Field>
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
}
