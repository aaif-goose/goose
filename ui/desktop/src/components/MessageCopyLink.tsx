/* global ClipboardItem */

import React, { useState } from 'react';
import { Copy } from './icons';
import { defineMessages, useIntl } from '../i18n';

const i18n = defineMessages({
  copied: {
    id: 'messageCopyLink.copied',
    defaultMessage: 'Copied!',
  },
  copy: {
    id: 'messageCopyLink.copy',
    defaultMessage: 'Copy',
  },
});

interface MessageCopyLinkProps {
  text: string;
  contentRef: React.RefObject<HTMLDivElement | null>;
}

export default function MessageCopyLink({ text, contentRef }: MessageCopyLinkProps) {
  const intl = useIntl();
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    try {
      if (contentRef?.current) {
        // Create a temporary container to handle HTML content
        const container = document.createElement('div');
        container.innerHTML = contentRef.current.innerHTML;

        // Clean up any copy buttons from the content
        const copyButtons = container.querySelectorAll('button');
        copyButtons.forEach((button) => button.remove());

        // Create the clipboard data
        const clipboardData = new ClipboardItem({
          'text/plain': new Blob([text], { type: 'text/plain' }),
          'text/html': new Blob([container.innerHTML], { type: 'text/html' }),
        });

        await navigator.clipboard.write([clipboardData]);
      } else {
        await navigator.clipboard.writeText(text);
      }

      setCopied(true);
      setTimeout(() => setCopied(false), 2000); // Reset after 2 seconds
    } catch (err) {
      console.error('Failed to copy text: ', err);
      // Fallback to plain text if HTML copy fails
      try {
        await navigator.clipboard.writeText(text);
        setCopied(true);
        setTimeout(() => setCopied(false), 2000);
      } catch (fallbackErr) {
        console.error('Failed to copy text (fallback): ', fallbackErr);
      }
    }
  };

  return (
    <button
      onClick={handleCopy}
      className="flex font-mono items-center gap-1 text-xs text-text-secondary hover:cursor-pointer hover:text-text-primary transition-all duration-200 opacity-0 group-hover:opacity-100 -translate-y-4 group-hover:translate-y-0"
    >
      <Copy className="h-3 w-3" />
      <span>{copied ? intl.formatMessage(i18n.copied) : intl.formatMessage(i18n.copy)}</span>
    </button>
  );
}
