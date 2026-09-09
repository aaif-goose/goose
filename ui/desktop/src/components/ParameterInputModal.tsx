import React, { useId, useState, useEffect } from 'react';
import { Parameter } from '../recipe';
import { Button } from './ui/button';
import { defineMessages, useIntl } from '../i18n';

const i18n = defineMessages({
  cancelRecipeSetup: {
    id: 'parameterInputModal.cancelRecipeSetup',
    defaultMessage: 'Cancel Recipe Setup',
  },
  whatToDo: {
    id: 'parameterInputModal.whatToDo',
    defaultMessage: 'What would you like to do?',
  },
  backToForm: {
    id: 'parameterInputModal.backToForm',
    defaultMessage: 'Back to Parameter Form',
  },
  startNewChat: {
    id: 'parameterInputModal.startNewChat',
    defaultMessage: 'Start New Chat (No Recipe)',
  },
  recipeParameters: {
    id: 'parameterInputModal.recipeParameters',
    defaultMessage: 'Recipe Parameters',
  },
  selectOption: {
    id: 'parameterInputModal.selectOption',
    defaultMessage: 'Select an option...',
  },
  select: {
    id: 'parameterInputModal.select',
    defaultMessage: 'Select...',
  },
  true: {
    id: 'parameterInputModal.true',
    defaultMessage: 'True',
  },
  false: {
    id: 'parameterInputModal.false',
    defaultMessage: 'False',
  },
  enterValue: {
    id: 'parameterInputModal.enterValue',
    defaultMessage: 'Enter value for {key}...',
  },
  cancel: {
    id: 'parameterInputModal.cancel',
    defaultMessage: 'Cancel',
  },
  startRecipe: {
    id: 'parameterInputModal.startRecipe',
    defaultMessage: 'Start Recipe',
  },
});

interface ParameterInputModalProps {
  parameters: Parameter[];
  onSubmit: (values: Record<string, string>) => void;
  onClose: () => void;
  initialValues?: Record<string, string>;
}

function needsUserValue(param: Parameter): boolean {
  return param.requirement === 'required' || param.requirement === 'user_prompt';
}

const NUMBER_VALUE_PATTERN = /^-?(?:\d+(?:\.\d+)?|\.\d+)(?:[eE][+-]?\d+)?$/;

function isValidParameterValue(param: Parameter, value: string): boolean {
  switch (param.input_type) {
    case 'select':
      return param.options?.includes(value) ?? true;
    case 'boolean':
      return value === 'true' || value === 'false';
    case 'number':
      return NUMBER_VALUE_PATTERN.test(value) && Number.isFinite(Number(value));
    default:
      return true;
  }
}

function createParameterValueMap(): Record<string, string> {
  return Object.create(null) as Record<string, string>;
}

const ParameterInputModal: React.FC<ParameterInputModalProps> = ({
  parameters,
  onSubmit,
  onClose,
  initialValues,
}) => {
  const intl = useIntl();
  const fieldIdPrefix = useId();
  const fieldId = (key: string): string => `${fieldIdPrefix}-${key}`;
  const [inputValues, setInputValues] = useState<Record<string, string>>(createParameterValueMap);
  const [validationErrors, setValidationErrors] =
    useState<Record<string, string>>(createParameterValueMap);
  const [showCancelOptions, setShowCancelOptions] = useState(false);

  useEffect(() => {
    const values = createParameterValueMap();
    parameters.forEach((param) => {
      if (param.input_type === 'file') {
        return;
      }

      if (param.requirement === 'optional' && param.default != null) {
        if (isValidParameterValue(param, param.default)) {
          values[param.key] = param.default;
        }
      }

      const initialValue =
        initialValues && Object.prototype.hasOwnProperty.call(initialValues, param.key)
          ? initialValues[param.key]
          : undefined;
      if (initialValue !== undefined && isValidParameterValue(param, initialValue)) {
        values[param.key] = initialValue;
      }
    });

    setInputValues(values);
  }, [parameters, initialValues]);

  const handleChange = (name: string, value: string): void => {
    setInputValues((prevValues: Record<string, string>) => {
      const values = Object.assign(createParameterValueMap(), prevValues);
      values[name] = value;
      return values;
    });
  };

  const handleSubmit = (e: React.SyntheticEvent): void => {
    e.preventDefault();
    setValidationErrors(createParameterValueMap());

    const errors = createParameterValueMap();
    const submittedValues = createParameterValueMap();

    parameters.forEach((param) => {
      const value = inputValues[param.key];
      if (needsUserValue(param) && !value?.trim()) {
        errors[param.key] = `${param.description || param.key} is required`;
        return;
      }

      if (value === undefined) {
        if (
          param.requirement === 'optional' &&
          param.default != null &&
          !isValidParameterValue(param, param.default)
        ) {
          errors[param.key] = `${param.description || param.key} has an invalid value`;
        }
        return;
      }

      if (!isValidParameterValue(param, value)) {
        errors[param.key] = `${param.description || param.key} has an invalid value`;
        return;
      }

      submittedValues[param.key] = value;
    });

    if (Object.keys(errors).length > 0) {
      setValidationErrors(errors);
      return;
    }

    onSubmit(submittedValues);
  };

  const handleCancel = (): void => {
    const hasAnyParams = parameters.length > 0;

    if (hasAnyParams) {
      setShowCancelOptions(true);
    } else {
      onClose();
    }
  };

  return (
    <div className="fixed inset-0 backdrop-blur-sm z-50 flex justify-center items-center animate-[fadein_200ms_ease-in]">
      {showCancelOptions ? (
        <div className="bg-background-primary border border-border-primary rounded-xl p-8 shadow-2xl w-full max-w-md">
          <h2 className="text-xl font-bold text-text-primary mb-4">
            {intl.formatMessage(i18n.cancelRecipeSetup)}
          </h2>
          <p className="text-text-primary mb-6">{intl.formatMessage(i18n.whatToDo)}</p>
          <div className="flex flex-col gap-3">
            <Button
              onClick={() => setShowCancelOptions(false)}
              variant="default"
              size="lg"
              className="w-full rounded-full"
            >
              {intl.formatMessage(i18n.backToForm)}
            </Button>
            <Button onClick={onClose} variant="outline" size="lg" className="w-full rounded-full">
              {intl.formatMessage(i18n.startNewChat)}
            </Button>
          </div>
        </div>
      ) : (
        <div className="bg-background-primary border border-border-primary rounded-xl shadow-2xl w-full max-w-lg max-h-[90vh] flex flex-col overflow-hidden">
          <div className="p-8 pb-4 flex-shrink-0">
            <h2 className="text-xl font-bold text-text-primary mb-6">
              {intl.formatMessage(i18n.recipeParameters)}
            </h2>
          </div>
          <div className="flex-1 overflow-y-auto px-8">
            <form onSubmit={handleSubmit} className="space-y-4 mb-4">
              {parameters.map((param) => (
                <div key={param.key}>
                  <label
                    htmlFor={fieldId(param.key)}
                    className="block text-md font-medium text-text-primary mb-2"
                  >
                    {param.description || param.key}
                    {needsUserValue(param) && <span className="text-red-500 ml-1">*</span>}
                  </label>

                  {param.input_type === 'select' && param.options ? (
                    <select
                      id={fieldId(param.key)}
                      value={inputValues[param.key] || ''}
                      onChange={(e) => handleChange(param.key, e.target.value)}
                      className={`w-full p-3 border rounded-lg bg-background-secondary text-text-primary focus:outline-none focus:ring-2 ${
                        validationErrors[param.key]
                          ? 'border-red-500 focus:ring-red-500'
                          : 'border-border-primary focus:ring-border-secondary'
                      }`}
                    >
                      <option value="">{intl.formatMessage(i18n.selectOption)}</option>
                      {param.options.map((option) => (
                        <option key={option} value={option}>
                          {option}
                        </option>
                      ))}
                    </select>
                  ) : param.input_type === 'boolean' ? (
                    <select
                      id={fieldId(param.key)}
                      value={inputValues[param.key] || ''}
                      onChange={(e) => handleChange(param.key, e.target.value)}
                      className={`w-full p-3 border rounded-lg bg-background-secondary text-text-primary focus:outline-none focus:ring-2 ${
                        validationErrors[param.key]
                          ? 'border-red-500 focus:ring-red-500'
                          : 'border-border-primary focus:ring-border-secondary'
                      }`}
                    >
                      <option value="">{intl.formatMessage(i18n.select)}</option>
                      <option value="true">{intl.formatMessage(i18n.true)}</option>
                      <option value="false">{intl.formatMessage(i18n.false)}</option>
                    </select>
                  ) : (
                    <input
                      id={fieldId(param.key)}
                      type={param.input_type === 'number' ? 'number' : 'text'}
                      value={inputValues[param.key] || ''}
                      onChange={(e) => handleChange(param.key, e.target.value)}
                      className={`w-full p-3 border rounded-lg bg-background-secondary text-text-primary focus:outline-none focus:ring-2 ${
                        validationErrors[param.key]
                          ? 'border-red-500 focus:ring-red-500'
                          : 'border-border-primary focus:ring-border-secondary'
                      }`}
                      placeholder={
                        param.default || intl.formatMessage(i18n.enterValue, { key: param.key })
                      }
                    />
                  )}

                  {validationErrors[param.key] && (
                    <p className="text-red-500 text-sm mt-1">{validationErrors[param.key]}</p>
                  )}
                </div>
              ))}
            </form>
          </div>
          <div className="p-8 pt-4 flex-shrink-0">
            <div className="flex justify-end gap-4">
              <Button
                type="button"
                onClick={handleCancel}
                variant="outline"
                size="default"
                className="rounded-full"
              >
                {intl.formatMessage(i18n.cancel)}
              </Button>
              <Button
                type="button"
                onClick={handleSubmit}
                variant="default"
                size="default"
                className="rounded-full"
              >
                {intl.formatMessage(i18n.startRecipe)}
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

export default ParameterInputModal;
