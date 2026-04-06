import React, { useState } from 'react';
import { Plus, Trash2 } from 'lucide-react';
import { Button } from '../../ui/button';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  addValue: {
    id: 'keyValueEditor.addValue',
    defaultMessage: 'Add pre-configured value',
  },
  removeValue: {
    id: 'keyValueEditor.removeValue',
    defaultMessage: 'Remove pre-configured value {key}',
  },
  defaultKeyPlaceholder: {
    id: 'keyValueEditor.defaultKeyPlaceholder',
    defaultMessage: 'Parameter name...',
  },
  defaultValuePlaceholder: {
    id: 'keyValueEditor.defaultValuePlaceholder',
    defaultMessage: 'Parameter value...',
  },
});

interface KeyValueEditorProps {
  values: Record<string, string>;
  onChange: (values: Record<string, string>) => void;
  keyPlaceholder?: string;
  valuePlaceholder?: string;
}

export default function KeyValueEditor({
  values,
  onChange,
  keyPlaceholder,
  valuePlaceholder,
}: KeyValueEditorProps) {
  const intl = useIntl();
  const [newKey, setNewKey] = useState('');
  const [newValue, setNewValue] = useState('');

  const handleAdd = () => {
    if (newKey.trim() && newValue.trim()) {
      onChange({ ...values, [newKey.trim()]: newValue.trim() });
      setNewKey('');
      setNewValue('');
    }
  };

  const handleRemove = (key: string) => {
    const updated = { ...values };
    delete updated[key];
    onChange(updated);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      handleAdd();
    }
  };

  return (
    <div>
      <div className="flex gap-2 mb-3">
        <input
          type="text"
          value={newKey}
          onChange={(e) => setNewKey(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={keyPlaceholder || intl.formatMessage(i18n.defaultKeyPlaceholder)}
          className="flex-1 px-3 py-2 border border-border-subtle rounded-lg bg-background-primary text-text-standard focus:outline-none focus:ring-2 focus:ring-ring text-sm"
        />
        <input
          type="text"
          value={newValue}
          onChange={(e) => setNewValue(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={valuePlaceholder || intl.formatMessage(i18n.defaultValuePlaceholder)}
          className="flex-1 px-3 py-2 border border-border-subtle rounded-lg bg-background-primary text-text-standard focus:outline-none focus:ring-2 focus:ring-ring text-sm"
        />
        <Button
          type="button"
          onClick={handleAdd}
          disabled={!newKey.trim() || !newValue.trim()}
          variant="outline"
          size="sm"
          className="px-3"
          aria-label={intl.formatMessage(i18n.addValue)}
        >
          <Plus className="w-4 h-4" />
        </Button>
      </div>

      {Object.keys(values).length > 0 && (
        <div className="space-y-2 border border-border-subtle rounded-lg p-3">
          {Object.entries(values).map(([key, value]) => (
            <div
              key={key}
              className="flex items-center justify-between p-2 bg-background-muted rounded"
            >
              <div className="flex-1">
                <span className="text-sm font-medium text-text-standard">{key}</span>
                <span className="text-sm text-text-muted mx-2">=</span>
                <span className="text-sm text-text-standard">{value}</span>
              </div>
              <Button
                type="button"
                onClick={() => handleRemove(key)}
                variant="ghost"
                size="sm"
                className="p-1 hover:bg-background-danger/10 hover:text-text-danger"
                aria-label={intl.formatMessage(i18n.removeValue, { key })}
                title={intl.formatMessage(i18n.removeValue, { key })}
              >
                <Trash2 className="w-4 h-4" />
              </Button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
