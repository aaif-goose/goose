import { useState } from 'react';
import { defineMessages, useIntl } from '../i18n';

const i18n = defineMessages({
  unableToLoad: {
    id: 'imagePreview.unableToLoad',
    defaultMessage: 'Unable to load image',
  },
  altText: {
    id: 'imagePreview.altText',
    defaultMessage: 'goose image',
  },
  clickToCollapse: {
    id: 'imagePreview.clickToCollapse',
    defaultMessage: 'Click to collapse',
  },
  clickToExpand: {
    id: 'imagePreview.clickToExpand',
    defaultMessage: 'Click to expand',
  },
});

interface ImagePreviewProps {
  src: string;
}

export default function ImagePreview({ src }: ImagePreviewProps) {
  const intl = useIntl();
  const [isExpanded, setIsExpanded] = useState(false);
  const [error, setError] = useState(false);

  if (error) {
    return (
      <div className="text-red-500 text-xs italic mt-1 mb-1">
        {intl.formatMessage(i18n.unableToLoad)}
      </div>
    );
  }

  return (
    <div className={`image-preview mt-2 mb-2`}>
      <img
        src={src}
        alt={intl.formatMessage(i18n.altText)}
        onError={() => setError(true)}
        onClick={() => setIsExpanded(!isExpanded)}
        className={`rounded border border-border-primary cursor-pointer hover:border-border-primary transition-all ${
          isExpanded ? 'max-w-full max-h-96' : 'max-h-40 max-w-40'
        }`}
        style={{ objectFit: 'contain' }}
      />
      <div className="text-xs text-text-secondary mt-1">
        {isExpanded
          ? intl.formatMessage(i18n.clickToCollapse)
          : intl.formatMessage(i18n.clickToExpand)}
      </div>
    </div>
  );
}
