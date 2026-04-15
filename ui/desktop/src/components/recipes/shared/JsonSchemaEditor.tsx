import React, { useState } from 'react';
import { Button } from '../../ui/button';
import { useEscapeKey } from '../../../hooks/useEscapeKey';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  title: {
    id: 'jsonSchemaEditor.title',
    defaultMessage: 'JSON Schema Editor',
  },
  label: {
    id: 'jsonSchemaEditor.label',
    defaultMessage: 'Response JSON Schema',
  },
  insertExample: {
    id: 'jsonSchemaEditor.insertExample',
    defaultMessage: 'Insert Example',
  },
  description: {
    id: 'jsonSchemaEditor.description',
    defaultMessage: "Define the expected structure of the AI's response using JSON Schema format",
  },
  invalidJson: {
    id: 'jsonSchemaEditor.invalidJson',
    defaultMessage: 'Invalid JSON format',
  },
  cancel: {
    id: 'jsonSchemaEditor.cancel',
    defaultMessage: 'Cancel',
  },
  save: {
    id: 'jsonSchemaEditor.save',
    defaultMessage: 'Save Schema',
  },
});

interface JsonSchemaEditorProps {
  isOpen: boolean;
  onClose: () => void;
  value: string;
  onChange: (value: string) => void;
  error?: string;
}

export default function JsonSchemaEditor({
  isOpen,
  onClose,
  value,
  onChange,
  error,
}: JsonSchemaEditorProps) {
  const intl = useIntl();
  const [localValue, setLocalValue] = useState(value);
  const [localError, setLocalError] = useState('');

  useEscapeKey(isOpen, onClose);

  React.useEffect(() => {
    if (isOpen) {
      setLocalValue(value);
      setLocalError('');
    }
  }, [isOpen, value]);

  const handleSave = () => {
    if (localValue.trim()) {
      try {
        JSON.parse(localValue.trim());
        setLocalError('');
      } catch {
        setLocalError(intl.formatMessage(i18n.invalidJson));
        return;
      }
    }

    onChange(localValue);
    onClose();
  };

  const handleCancel = () => {
    setLocalValue(value);
    setLocalError('');
    onClose();
  };

  const insertExample = () => {
    const example = `{
  "type": "object",
  "properties": {
    "result": {
      "type": "string",
      "description": "The main result"
    },
    "status": {
      "type": "string",
      "enum": ["success", "error"],
      "description": "Operation status"
    },
    "data": {
      "type": "object",
      "properties": {
        "items": {
          "type": "array",
          "items": {
            "type": "string"
          }
        }
      }
    }
  },
  "required": ["result", "status"]
}`;
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
      <div className="bg-background-primary border border-border-primary rounded-lg p-6 w-[800px] max-w-[90vw] max-h-[90vh] overflow-hidden flex flex-col">
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
              <label className="block text-sm font-medium text-text-primary">
                {intl.formatMessage(i18n.label)}
              </label>
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
              {intl.formatMessage(i18n.description)}
            </p>
          </div>

          <div className="flex-1 min-h-0">
            <textarea
              value={localValue}
              onChange={(e) => {
                setLocalValue(e.target.value);
                setLocalError('');
              }}
              className={`w-full h-full min-h-[400px] p-3 border rounded-lg bg-background-primary text-text-primary focus:outline-none focus:ring-2 focus:ring-blue-500 resize-none font-mono text-sm ${
                localError || error ? 'border-red-500' : 'border-border-primary'
              }`}
              placeholder={`{
  "type": "object",
  "properties": {
    "result": {
      "type": "string",
      "description": "The main result"
    }
  },
  "required": ["result"]
}`}
            />
            {(localError || error) && (
              <p className="text-red-500 text-sm mt-2">{localError || error}</p>
            )}
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
