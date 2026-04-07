import React, { useState, useCallback } from 'react';
import { Input } from './input';
import { Button } from './button';
import { defineMessages, useIntl } from '../../i18n';

const i18n = defineMessages({
  submit: {
    id: 'jsonSchemaForm.submit',
    defaultMessage: 'Submit',
  },
  cancel: {
    id: 'jsonSchemaForm.cancel',
    defaultMessage: 'Cancel',
  },
  fieldRequired: {
    id: 'jsonSchemaForm.fieldRequired',
    defaultMessage: 'This field is required',
  },
  minLength: {
    id: 'jsonSchemaForm.minLength',
    defaultMessage: 'Minimum length is {minLength}',
  },
  maxLength: {
    id: 'jsonSchemaForm.maxLength',
    defaultMessage: 'Maximum length is {maxLength}',
  },
  minValue: {
    id: 'jsonSchemaForm.minValue',
    defaultMessage: 'Minimum value is {minimum}',
  },
  maxValue: {
    id: 'jsonSchemaForm.maxValue',
    defaultMessage: 'Maximum value is {maximum}',
  },
  selectPlaceholder: {
    id: 'jsonSchemaForm.selectPlaceholder',
    defaultMessage: 'Select...',
  },
  noFields: {
    id: 'jsonSchemaForm.noFields',
    defaultMessage: 'No fields to display',
  },
});

interface JsonSchemaProperty {
  type?: string;
  description?: string;
  default?: unknown;
  enum?: string[];
  minimum?: number;
  maximum?: number;
  minLength?: number;
  maxLength?: number;
}

export interface JsonSchema {
  type?: string;
  properties?: Record<string, JsonSchemaProperty>;
  required?: string[];
  title?: string;
  description?: string;
}

interface JsonSchemaFormProps {
  schema: JsonSchema;
  onSubmit: (data: Record<string, unknown>) => void;
  onCancel?: () => void;
  submitLabel?: string;
  cancelLabel?: string;
  disabled?: boolean;
}

export default function JsonSchemaForm({
  schema,
  onSubmit,
  onCancel,
  submitLabel,
  cancelLabel,
  disabled = false,
}: JsonSchemaFormProps) {
  const intl = useIntl();
  const [formData, setFormData] = useState<Record<string, unknown>>(() => {
    const initial: Record<string, unknown> = {};
    if (schema.properties) {
      for (const [key, prop] of Object.entries(schema.properties)) {
        if (prop.default !== undefined) {
          initial[key] = prop.default;
        } else if (prop.type === 'boolean') {
          initial[key] = false;
        } else if (prop.type === 'number' || prop.type === 'integer') {
          initial[key] = prop.minimum ?? 0;
        } else {
          initial[key] = '';
        }
      }
    }
    return initial;
  });

  const [errors, setErrors] = useState<Record<string, string>>({});

  const validateField = useCallback(
    (key: string, value: unknown): string | null => {
      const prop = schema.properties?.[key];
      if (!prop) return null;

      const isRequired = schema.required?.includes(key);

      if (isRequired && (value === '' || value === null || value === undefined)) {
        return intl.formatMessage(i18n.fieldRequired);
      }

      if (prop.type === 'string' && typeof value === 'string') {
        if (!isRequired && value === '') return null;

        if (prop.minLength !== undefined && value.length < prop.minLength) {
          return intl.formatMessage(i18n.minLength, { minLength: prop.minLength });
        }
        if (prop.maxLength !== undefined && value.length > prop.maxLength) {
          return intl.formatMessage(i18n.maxLength, { maxLength: prop.maxLength });
        }
      }

      if ((prop.type === 'number' || prop.type === 'integer') && typeof value === 'number') {
        if (prop.minimum !== undefined && value < prop.minimum) {
          return intl.formatMessage(i18n.minValue, { minimum: prop.minimum });
        }
        if (prop.maximum !== undefined && value > prop.maximum) {
          return intl.formatMessage(i18n.maxValue, { maximum: prop.maximum });
        }
      }

      return null;
    },
    [schema, intl]
  );

  const handleChange = useCallback(
    (key: string, value: unknown) => {
      setFormData((prev) => ({ ...prev, [key]: value }));

      const error = validateField(key, value);
      setErrors((prev) => {
        if (error) {
          return { ...prev, [key]: error };
        }
        const newErrors = { ...prev };
        delete newErrors[key];
        return newErrors;
      });
    },
    [validateField]
  );

  const handleSubmit = useCallback(
    (e: React.FormEvent) => {
      e.preventDefault();

      const newErrors: Record<string, string> = {};
      if (schema.properties) {
        for (const key of Object.keys(schema.properties)) {
          const error = validateField(key, formData[key]);
          if (error) {
            newErrors[key] = error;
          }
        }
      }

      if (Object.keys(newErrors).length > 0) {
        setErrors(newErrors);
        return;
      }

      onSubmit(formData);
    },
    [formData, onSubmit, schema.properties, validateField]
  );

  const renderField = (key: string, prop: JsonSchemaProperty) => {
    const value = formData[key];
    const error = errors[key];
    const isRequired = schema.required?.includes(key);

    if (prop.enum) {
      return (
        <select
          id={key}
          value={String(value ?? '')}
          onChange={(e) => handleChange(key, e.target.value)}
          disabled={disabled}
          className="flex h-9 w-full rounded-md border focus:border-border-secondary hover:border-border-secondary bg-background-primary px-3 py-1 text-base transition-colors focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50 md:text-sm"
        >
          {!isRequired && <option value="">{intl.formatMessage(i18n.selectPlaceholder)}</option>}
          {prop.enum.map((option) => (
            <option key={option} value={option}>
              {option}
            </option>
          ))}
        </select>
      );
    }

    if (prop.type === 'boolean') {
      return (
        <label className="flex items-center gap-2 cursor-pointer">
          <input
            type="checkbox"
            id={key}
            checked={Boolean(value)}
            onChange={(e) => handleChange(key, e.target.checked)}
            disabled={disabled}
            className="h-4 w-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500"
          />
          <span className="text-sm text-text-primary">{prop.description || key}</span>
        </label>
      );
    }

    if (prop.type === 'number' || prop.type === 'integer') {
      return (
        <Input
          type="number"
          id={key}
          value={String(value ?? '')}
          onChange={(e) => {
            const numValue =
              prop.type === 'integer' ? parseInt(e.target.value, 10) : parseFloat(e.target.value);
            handleChange(key, isNaN(numValue) ? '' : numValue);
          }}
          min={prop.minimum}
          max={prop.maximum}
          step={prop.type === 'integer' ? 1 : 'any'}
          disabled={disabled}
          className={error ? 'border-red-500' : ''}
        />
      );
    }

    return (
      <Input
        type="text"
        id={key}
        value={String(value ?? '')}
        onChange={(e) => handleChange(key, e.target.value)}
        minLength={prop.minLength}
        maxLength={prop.maxLength}
        disabled={disabled}
        className={error ? 'border-red-500' : ''}
      />
    );
  };

  if (!schema.properties || Object.keys(schema.properties).length === 0) {
    return <div className="text-text-secondary text-sm">{intl.formatMessage(i18n.noFields)}</div>;
  }

  return (
    <form onSubmit={handleSubmit} className="flex flex-col gap-4">
      {Object.entries(schema.properties).map(([key, prop]) => {
        const isRequired = schema.required?.includes(key);
        const error = errors[key];

        if (prop.type === 'boolean') {
          return (
            <div key={key} className="flex flex-col gap-1">
              {renderField(key, prop)}
              {error && <span className="text-red-500 text-xs">{error}</span>}
            </div>
          );
        }

        return (
          <div key={key} className="flex flex-col gap-1">
            <label htmlFor={key} className="text-sm font-medium text-text-primary">
              {key}
              {isRequired && <span className="text-red-500 ml-1">*</span>}
            </label>
            {prop.description && prop.type !== 'boolean' && (
              <span className="text-xs text-text-secondary">{prop.description}</span>
            )}
            {renderField(key, prop)}
            {error && <span className="text-red-500 text-xs">{error}</span>}
          </div>
        );
      })}

      <div className="flex gap-2 mt-2">
        <Button type="submit" disabled={disabled}>
          {submitLabel ?? intl.formatMessage(i18n.submit)}
        </Button>
        {onCancel && (
          <Button type="button" variant="outline" onClick={onCancel} disabled={disabled}>
            {cancelLabel ?? intl.formatMessage(i18n.cancel)}
          </Button>
        )}
      </div>
    </form>
  );
}
