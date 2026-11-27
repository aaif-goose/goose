import { useEffect, useState } from 'react';
import { useConfig } from '../../ConfigContext';
import { defineMessages, useIntl } from '../../../i18n';
import { Input } from '../../ui/input';

const CONFIG_KEY = 'GOOSE_READ_IMAGE_MAX_DIMENSION';
const DEFAULT_MAX_DIMENSION = 1024;
const MIN_MAX_DIMENSION = 64;
const MAX_MAX_DIMENSION = 8192;

const i18n = defineMessages({
  title: {
    id: 'imageSettings.title',
    defaultMessage: 'Image processing',
  },
  description: {
    id: 'imageSettings.description',
    defaultMessage:
      'Images loaded with Read Image are downscaled to this maximum width or height before being sent to the model. Original dimensions remain available for cropping.',
  },
  maximumDimension: {
    id: 'imageSettings.maximumDimension',
    defaultMessage: 'Maximum dimension (pixels)',
  },
});

export function ImageSettings() {
  const intl = useIntl();
  const { read, upsert } = useConfig();
  const [savedValue, setSavedValue] = useState(DEFAULT_MAX_DIMENSION);
  const [value, setValue] = useState(DEFAULT_MAX_DIMENSION.toString());

  useEffect(() => {
    let active = true;

    read(CONFIG_KEY, false).then((configured) => {
      if (!active) return;
      const parsed = Number(configured);
      const next =
        Number.isInteger(parsed) && parsed >= MIN_MAX_DIMENSION && parsed <= MAX_MAX_DIMENSION
          ? parsed
          : DEFAULT_MAX_DIMENSION;
      setSavedValue(next);
      setValue(next.toString());
    });

    return () => {
      active = false;
    };
  }, [read]);

  const save = async () => {
    const parsed = Number(value);
    if (!Number.isInteger(parsed) || parsed < MIN_MAX_DIMENSION || parsed > MAX_MAX_DIMENSION) {
      setValue(savedValue.toString());
      return;
    }

    await upsert(CONFIG_KEY, parsed, false);
    setSavedValue(parsed);
    setValue(parsed.toString());
  };

  return (
    <div className="flex items-center justify-between gap-6 py-2">
      <div>
        <h3 className="text-text-primary">{intl.formatMessage(i18n.title)}</h3>
        <p className="mt-[2px] max-w-md text-xs text-text-secondary">
          {intl.formatMessage(i18n.description)}
        </p>
      </div>
      <label className="flex shrink-0 flex-col gap-1 text-xs text-text-secondary">
        <span>{intl.formatMessage(i18n.maximumDimension)}</span>
        <Input
          type="number"
          min={MIN_MAX_DIMENSION}
          max={MAX_MAX_DIMENSION}
          step={64}
          value={value}
          onChange={(event) => setValue(event.target.value)}
          onBlur={() => void save()}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              event.currentTarget.blur();
            }
          }}
          className="w-28"
        />
      </label>
    </div>
  );
}
