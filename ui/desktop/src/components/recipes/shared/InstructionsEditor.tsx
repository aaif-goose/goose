import React, { useState } from 'react';
import { Button } from '../../ui/button';
import { useEscapeKey } from '../../../hooks/useEscapeKey';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  title: {
    id: 'instructionsEditor.title',
    defaultMessage: 'Instructions Editor',
  },
  label: {
    id: 'instructionsEditor.label',
    defaultMessage: 'Instructions',
  },
  insertExample: {
    id: 'instructionsEditor.insertExample',
    defaultMessage: 'Insert Example',
  },
  syntaxHelp: {
    id: 'instructionsEditor.syntaxHelp',
    defaultMessage: 'Use {code} syntax to define parameters that users can fill in',
  },
  placeholder: {
    id: 'instructionsEditor.placeholder',
    defaultMessage: 'Detailed instructions for the AI, hidden from the user',
  },
  cancel: {
    id: 'instructionsEditor.cancel',
    defaultMessage: 'Cancel',
  },
  save: {
    id: 'instructionsEditor.save',
    defaultMessage: 'Save Instructions',
  },
});

interface InstructionsEditorProps {
  isOpen: boolean;
  onClose: () => void;
  value: string;
  onChange: (value: string) => void;
  error?: string;
}

export default function InstructionsEditor({
  isOpen,
  onClose,
  value,
  onChange,
  error,
}: InstructionsEditorProps) {
  const intl = useIntl();
  const [localValue, setLocalValue] = useState(value);
  useEscapeKey(isOpen, onClose);

  React.useEffect(() => {
    if (isOpen) {
      setLocalValue(value);
    }
  }, [isOpen, value]);

  const handleSave = () => {
    onChange(localValue);
    onClose();
  };

  const handleCancel = () => {
    setLocalValue(value); // Reset to original value
    onClose();
  };

  const insertExample = () => {
    const example = `You are an AI assistant helping with {{task_type}}. 

Please follow these steps:
1. Analyze the provided {{input_data}}
2. Apply the specified {{methodology}} 
3. Generate a comprehensive report

Requirements:
- Be thorough and accurate
- Use clear, professional language
- Include specific examples where relevant
- Provide actionable recommendations

Format your response with:
- Executive summary
- Detailed analysis
- Key findings
- Next steps

Use {{parameter_name}} syntax for any user-provided values.`;
    setLocalValue(example);
  };

  if (!isOpen) return null;

  return (
    <div
      className="fixed inset-0 z-[400] flex items-center justify-center bg-black/50"
      onClick={(e) => {
        // Close modal when clicking backdrop
        if (e.target === e.currentTarget) {
          handleCancel();
        }
      }}
    >
      <div className="bg-background-primary border border-border-primary rounded-lg p-6 w-[900px] max-w-[90vw] max-h-[90vh] overflow-hidden flex flex-col">
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-lg font-medium text-text-primary">{intl.formatMessage(i18n.title)}</h3>
          <button
            type="button"
            onClick={handleCancel}
            className="text-text-secondary hover:text-text-primary text-2xl leading-none"
          >
            ×
          </button>
        </div>

        <div className="flex-1 flex flex-col min-h-0">
          <div className="mb-4">
            <div className="flex items-center justify-between mb-2">
              <label className="block text-sm font-medium text-text-primary">{intl.formatMessage(i18n.label)}</label>
              <Button
                type="button"
                onClick={insertExample}
                variant="ghost"
                size="sm"
                className="text-xs"
              >
                {intl.formatMessage(i18n.insertExample)}
              </Button>
            </div>
            <p className="text-xs text-text-secondary mb-3">
              {intl.formatMessage(i18n.syntaxHelp, { code: '{{parameter_name}}' })}
            </p>
          </div>

          <div className="flex-1 min-h-0">
            <textarea
              value={localValue}
              onChange={(e) => setLocalValue(e.target.value)}
              className={`w-full h-full min-h-[500px] p-3 border rounded-lg bg-background-primary text-text-primary focus:outline-none focus:ring-2 focus:ring-blue-500 resize-none font-mono text-sm ${
                error ? 'border-red-500' : 'border-border-primary'
              }`}
              placeholder={intl.formatMessage(i18n.placeholder)}
            />
            {error && <p className="text-red-500 text-sm mt-2">{error}</p>}
          </div>
        </div>

        <div className="flex justify-end space-x-3 mt-6 pt-4 border-t border-border-primary">
          <Button type="button" onClick={handleCancel} variant="ghost">
            {intl.formatMessage(i18n.cancel)}
          </Button>
          <Button type="button" onClick={handleSave} variant="default">
            {intl.formatMessage(i18n.save)}
          </Button>
        </div>
      </div>
    </div>
  );
}
