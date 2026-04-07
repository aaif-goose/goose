import { Input } from '../../../ui/input';
import { defineMessages, useIntl } from '../../../../i18n';

const i18n = defineMessages({
  timeoutLabel: {
    id: 'extensionTimeoutField.timeoutLabel',
    defaultMessage: 'Timeout',
  },
});

interface ExtensionTimeoutFieldProps {
  timeout: number;
  onChange: (key: string, value: string | number) => void;
  submitAttempted: boolean;
}

export default function ExtensionTimeoutField({
  timeout,
  onChange,
  submitAttempted,
}: ExtensionTimeoutFieldProps) {
  const isTimeoutValid = () => {
    // Check if timeout is not undefined, null, or empty string
    if (timeout === undefined || timeout === null) {
      return false;
    }

    // Convert to number if it's a string
    const timeoutValue = typeof timeout === 'string' ? Number(timeout) : timeout;

    // Check if it's a valid number (not NaN) and is a positive number
    return !isNaN(timeoutValue) && timeoutValue > 0;
  };

  const intl = useIntl();

  return (
    <div className="flex flex-col gap-4 mb-6">
      {/* Row with Timeout and timeout input side by side */}
      <div className="flex flex-col">
        <div className="flex-1">
          <label className="text-sm font-medium mb-2 block text-text-primary">{intl.formatMessage(i18n.timeoutLabel)}</label>
        </div>

        <Input
          value={timeout}
          onChange={(e) => onChange('timeout', e.target.value)}
          defaultValue={300}
          className={`${!submitAttempted || isTimeoutValid() ? 'border-border-primary' : 'border-red-500'} text-text-primary focus:border-border-primary`}
        />
        {submitAttempted && !isTimeoutValid() && (
          <div className="absolute text-xs text-red-500 mt-1">Timeout </div>
        )}
      </div>
    </div>
  );
}
