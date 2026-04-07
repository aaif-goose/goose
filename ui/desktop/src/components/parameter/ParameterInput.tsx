import React from 'react';
import { AlertTriangle, Trash2, ChevronDown, ChevronRight } from 'lucide-react';
import { Parameter } from '../../recipe';
import { defineMessages, useIntl } from '../../i18n';

const i18n = defineMessages({
  unusedWarningTitle: {
    id: 'parameterInput.unusedWarningTitle',
    defaultMessage:
      'This parameter is not used in the instructions or prompt. It will be available for manual input but may not be needed.',
  },
  unused: {
    id: 'parameterInput.unused',
    defaultMessage: 'Unused',
  },
  deleteParameter: {
    id: 'parameterInput.deleteParameter',
    defaultMessage: 'Delete parameter: {key}',
  },
  description: {
    id: 'parameterInput.description',
    defaultMessage: 'description',
  },
  descriptionPlaceholder: {
    id: 'parameterInput.descriptionPlaceholder',
    defaultMessage: 'E.g., "Enter the name for the new component"',
  },
  descriptionHelp: {
    id: 'parameterInput.descriptionHelp',
    defaultMessage: 'This is the message the end-user will see.',
  },
  inputType: {
    id: 'parameterInput.inputType',
    defaultMessage: 'Input Type',
  },
  typeString: {
    id: 'parameterInput.typeString',
    defaultMessage: 'String',
  },
  typeSelect: {
    id: 'parameterInput.typeSelect',
    defaultMessage: 'Select',
  },
  typeNumber: {
    id: 'parameterInput.typeNumber',
    defaultMessage: 'Number',
  },
  typeBoolean: {
    id: 'parameterInput.typeBoolean',
    defaultMessage: 'Boolean',
  },
  requirement: {
    id: 'parameterInput.requirement',
    defaultMessage: 'Requirement',
  },
  required: {
    id: 'parameterInput.required',
    defaultMessage: 'Required',
  },
  optional: {
    id: 'parameterInput.optional',
    defaultMessage: 'Optional',
  },
  defaultValue: {
    id: 'parameterInput.defaultValue',
    defaultMessage: 'Default Value',
  },
  defaultValuePlaceholder: {
    id: 'parameterInput.defaultValuePlaceholder',
    defaultMessage: 'Enter default value',
  },
  optionsLabel: {
    id: 'parameterInput.optionsLabel',
    defaultMessage: 'Options (one per line)',
  },
  optionsPlaceholder: {
    id: 'parameterInput.optionsPlaceholder',
    defaultMessage: 'Option 1\nOption 2\nOption 3',
  },
  optionsHelp: {
    id: 'parameterInput.optionsHelp',
    defaultMessage: 'Enter each option on a new line. These will be shown as dropdown choices.',
  },
});

interface ParameterInputProps {
  parameter: Parameter;
  onChange: (name: string, updatedParameter: Partial<Parameter>) => void;
  onDelete?: (parameterKey: string) => void;
  isUnused?: boolean;
  isExpanded?: boolean;
  onToggleExpanded?: (parameterKey: string) => void;
}

const ParameterInput: React.FC<ParameterInputProps> = ({
  parameter,
  onChange,
  onDelete,
  isUnused = false,
  isExpanded = true,
  onToggleExpanded,
}) => {
  const intl = useIntl();
  const { key, description, requirement } = parameter;
  const defaultValue = parameter.default || '';

  const handleToggleExpanded = (e: React.MouseEvent) => {
    // Only toggle if we're not clicking on the delete button
    if (onToggleExpanded && !(e.target as HTMLElement).closest('button')) {
      onToggleExpanded(key);
    }
  };

  return (
    <div className="parameter-input my-4 border rounded-lg bg-background-secondary shadow-sm relative">
      {/* Collapsed header - always visible */}
      <div
        className={`flex items-center justify-between p-4 ${onToggleExpanded ? 'cursor-pointer hover:bg-background-primary/50' : ''} transition-colors`}
        onClick={handleToggleExpanded}
      >
        <div className="flex items-center gap-2 flex-1">
          {onToggleExpanded && (
            <button
              type="button"
              className="p-1 hover:bg-background-primary rounded transition-colors"
              onClick={(e) => {
                e.stopPropagation();
                onToggleExpanded(key);
              }}
            >
              {isExpanded ? (
                <ChevronDown className="w-4 h-4 text-text-secondary" />
              ) : (
                <ChevronRight className="w-4 h-4 text-text-secondary" />
              )}
            </button>
          )}

          <div className="flex items-center gap-2">
            <span className="text-md font-bold text-text-primary">
              <code className="bg-background-primary px-2 py-1 rounded-md">{parameter.key}</code>
            </span>
            {isUnused && (
              <div
                className="flex items-center gap-1"
                title={intl.formatMessage(i18n.unusedWarningTitle)}
              >
                <AlertTriangle className="w-4 h-4 text-orange-500" />
                <span className="text-xs text-orange-500 font-normal">{intl.formatMessage(i18n.unused)}</span>
              </div>
            )}
          </div>
        </div>

        {onDelete && (
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              onDelete(key);
            }}
            className="p-1 text-red-500 hover:text-red-700 hover:bg-red-50 rounded transition-colors"
            title={intl.formatMessage(i18n.deleteParameter, { key })}
          >
            <Trash2 className="w-4 h-4" />
          </button>
        )}
      </div>

      {/* Expandable content - only shown when expanded */}
      {isExpanded && (
        <div className="px-4 pb-4 border-t border-border-primary">
          <div className="pt-4">
            <div className="mb-4">
              <label className="block text-md text-text-primary mb-2 font-semibold">
                {intl.formatMessage(i18n.description)}
              </label>
              <input
                type="text"
                value={description || ''}
                onChange={(e) => onChange(key, { description: e.target.value })}
                className="w-full p-3 border rounded-lg bg-background-primary text-text-primary focus:outline-none focus:ring-2 focus:ring-border-secondary"
                placeholder={intl.formatMessage(i18n.descriptionPlaceholder)}
              />
              <p className="text-sm text-text-secondary mt-1">
                {intl.formatMessage(i18n.descriptionHelp)}
              </p>
            </div>

            {/* Controls for requirement, input type, and default value */}
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              <div>
                <label className="block text-md text-text-primary mb-2 font-semibold">
                  {intl.formatMessage(i18n.inputType)}
                </label>
                <select
                  className="w-full p-3 border rounded-lg bg-background-primary text-text-primary"
                  value={parameter.input_type || 'string'}
                  onChange={(e) =>
                    onChange(key, { input_type: e.target.value as Parameter['input_type'] })
                  }
                >
                  <option value="string">{intl.formatMessage(i18n.typeString)}</option>
                  <option value="select">{intl.formatMessage(i18n.typeSelect)}</option>
                  <option value="number">{intl.formatMessage(i18n.typeNumber)}</option>
                  <option value="boolean">{intl.formatMessage(i18n.typeBoolean)}</option>
                </select>
              </div>

              <div>
                <label className="block text-md text-text-primary mb-2 font-semibold">
                  {intl.formatMessage(i18n.requirement)}
                </label>
                <select
                  className="w-full p-3 border rounded-lg bg-background-primary text-text-primary"
                  value={requirement}
                  onChange={(e) =>
                    onChange(key, { requirement: e.target.value as Parameter['requirement'] })
                  }
                >
                  <option value="required">{intl.formatMessage(i18n.required)}</option>
                  <option value="optional">{intl.formatMessage(i18n.optional)}</option>
                </select>
              </div>

              {/* The default value input is only shown for optional parameters */}
              {requirement === 'optional' && (
                <div>
                  <label className="block text-md text-text-primary mb-2 font-semibold">
                    {intl.formatMessage(i18n.defaultValue)}
                  </label>
                  <input
                    type="text"
                    value={defaultValue}
                    onChange={(e) => onChange(key, { default: e.target.value })}
                    className="w-full p-3 border rounded-lg bg-background-primary text-text-primary"
                    placeholder={intl.formatMessage(i18n.defaultValuePlaceholder)}
                  />
                </div>
              )}
            </div>

            {/* Options field for select input type */}
            {parameter.input_type === 'select' && (
              <div className="mt-4">
                <label className="block text-md text-text-primary mb-2 font-semibold">
                  {intl.formatMessage(i18n.optionsLabel)}
                </label>
                <textarea
                  value={(parameter.options || []).join('\n')}
                  onChange={(e) => {
                    // Don't filter out empty lines - preserve them so user can type on new lines
                    const options = e.target.value.split('\n');
                    onChange(key, { options });
                  }}
                  onKeyDown={(e) => {
                    // Allow Enter key to work normally in textarea (prevent form submission or modal close)
                    if (e.key === 'Enter') {
                      e.stopPropagation();
                    }
                  }}
                  className="w-full p-3 border rounded-lg bg-background-primary text-text-primary focus:outline-none focus:ring-2 focus:ring-border-secondary"
                  placeholder={intl.formatMessage(i18n.optionsPlaceholder)}
                  rows={4}
                />
                <p className="text-sm text-text-secondary mt-1">
                  {intl.formatMessage(i18n.optionsHelp)}
                </p>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
};

export default ParameterInput;
