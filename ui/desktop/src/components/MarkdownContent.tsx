import React, { useState, useEffect, useRef, memo, useMemo, useCallback } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import remarkBreaks from 'remark-breaks';
import remarkMath from 'remark-math';
import rehypeKatex from 'rehype-katex';
import 'katex/dist/katex.min.css';
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter';
import { oneDark } from 'react-syntax-highlighter/dist/esm/styles/prism';
// Improved oneDark theme for better comment contrast and readability
const customOneDarkTheme = {
  ...oneDark,
  'code[class*="language-"]': {
    ...oneDark['code[class*="language-"]'],
    color: '#e6e6e6',
    fontSize: '14px',
  },
  'pre[class*="language-"]': {
    ...oneDark['pre[class*="language-"]'],
    color: '#e6e6e6',
    fontSize: '14px',
  },
  comment: { ...oneDark.comment, color: '#a0a0a0', fontStyle: 'italic' },
  prolog: { ...oneDark.prolog, color: '#a0a0a0' },
  doctype: { ...oneDark.doctype, color: '#a0a0a0' },
  cdata: { ...oneDark.cdata, color: '#a0a0a0' },
};

import { Check, Copy } from './icons';
import { wrapHTMLInCodeBlock } from '../utils/htmlSecurity';
import { isProtocolSafe, getProtocol, BLOCKED_PROTOCOLS } from '../utils/urlSecurity';
import { ConfirmationModal } from './ui/ConfirmationModal';
import { defineMessages, useIntl } from '../i18n';

const i18n = defineMessages({
  copyCode: {
    id: 'markdownContent.copyCode',
    defaultMessage: 'Copy code',
  },
  openExternalLink: {
    id: 'markdownContent.openExternalLink',
    defaultMessage: 'Open External Link',
  },
  openProtocolLink: {
    id: 'markdownContent.openProtocolLink',
    defaultMessage: 'Open {protocol} link?',
  },
  thisWillOpen: {
    id: 'markdownContent.thisWillOpen',
    defaultMessage: 'This will open: {href}',
  },
  open: {
    id: 'markdownContent.open',
    defaultMessage: 'Open',
  },
  cancel: {
    id: 'markdownContent.cancel',
    defaultMessage: 'Cancel',
  },
  failedToOpenLink: {
    id: 'markdownContent.failedToOpenLink',
    defaultMessage: 'Failed to Open Link',
  },
  noApplicationFound: {
    id: 'markdownContent.noApplicationFound',
    defaultMessage: 'No application found to open this link.',
  },
});

interface CodeProps extends React.ClassAttributes<HTMLElement>, React.HTMLAttributes<HTMLElement> {
  inline?: boolean;
}

interface MarkdownContentProps {
  content: string;
  className?: string;
}

// Memoized CodeBlock component to prevent re-rendering when props haven't changed
const CodeBlock = memo(function CodeBlock({
  language,
  children,
}: {
  language: string;
  children: string;
}) {
  const intl = useIntl();
  const [copied, setCopied] = useState(false);
  const timeoutRef = useRef<number | null>(null);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(children);
      setCopied(true);

      if (timeoutRef.current) {
        window.clearTimeout(timeoutRef.current);
      }

      timeoutRef.current = window.setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      console.error('Failed to copy text: ', err);
    }
  };

  useEffect(() => {
    return () => {
      if (timeoutRef.current) {
        window.clearTimeout(timeoutRef.current);
      }
    };
  }, []);

  // Memoize the SyntaxHighlighter component to prevent re-rendering
  // Only re-render if language or children change
  const memoizedSyntaxHighlighter = useMemo(() => {
    // For very large code blocks, consider truncating or lazy loading
    const isLargeCodeBlock = children.length > 10000; // 10KB threshold

    if (isLargeCodeBlock) {
      console.log(`Large code block detected (${children.length} chars), consider optimization`);
    }

    return (
      <SyntaxHighlighter
        style={customOneDarkTheme}
        language={language}
        PreTag="div"
        customStyle={{
          margin: 0,
          width: '100%',
          maxWidth: '100%',
        }}
        codeTagProps={{
          style: {
            whiteSpace: 'pre-wrap',
            wordBreak: 'break-all',
            overflowWrap: 'break-word',
            fontFamily: 'var(--font-mono)',
            fontSize: '14px',
          },
        }}
        // Performance optimizations for SyntaxHighlighter
        showLineNumbers={false} // Disable line numbers for better performance
        wrapLines={false} // Disable line wrapping for better performance
        lineProps={undefined} // Don't add extra props to each line
      >
        {children}
      </SyntaxHighlighter>
    );
  }, [language, children]);

  return (
    <div className="relative group w-full">
      <button
        onClick={handleCopy}
        className="absolute right-2 bottom-2 p-1.5 rounded-lg bg-gray-700/50 text-gray-300 font-sans text-sm
                 opacity-0 group-hover:opacity-100 transition-opacity duration-200
                 hover:bg-gray-600/50 hover:text-gray-100 z-10"
        title={intl.formatMessage(i18n.copyCode)}
      >
        {copied ? <Check className="h-4 w-4" /> : <Copy className="h-4 w-4" />}
      </button>
      <div className="w-full overflow-x-auto">{memoizedSyntaxHighlighter}</div>
    </div>
  );
});

const MarkdownCode = memo(
  React.forwardRef(function MarkdownCode(
    { inline, className, children, ...props }: CodeProps,
    ref: React.Ref<HTMLElement>
  ) {
    const match = /language-(\w+)/.exec(className || '');
    return !inline && match ? (
      <CodeBlock language={match[1]}>{String(children).replace(/\n$/, '')}</CodeBlock>
    ) : (
      <code ref={ref} {...props} className="break-all bg-inline-code whitespace-pre-wrap font-mono">
        {children}
      </code>
    );
  })
);

// Custom URL transform to preserve deep link URLs (spotify:, vscode:, slack:, etc.)
// React-markdown's default only allows http/https/mailto and strips all other protocols
// We allow all protocols except dangerous ones (javascript:, data:, file:, etc.)
const customUrlTransform = (url: string): string => {
  try {
    const protocol = new URL(url).protocol;
    if (BLOCKED_PROTOCOLS.includes(protocol)) {
      return '';
    }
  } catch {
    // Not a valid URL, allow it (could be relative path)
  }
  return url;
};

const MarkdownContent = memo(function MarkdownContent({
  content,
  className = '',
}: MarkdownContentProps) {
  const intl = useIntl();
  const [processedContent, setProcessedContent] = useState(content);
  const [pendingLink, setPendingLink] = useState<{ protocol: string; href: string } | null>(null);

  useEffect(() => {
    try {
      const processed = wrapHTMLInCodeBlock(content);
      setProcessedContent(processed);
    } catch (error) {
      console.error('Error processing content:', error);
      setProcessedContent(content);
    }
  }, [content]);

  const handleConfirmOpen = useCallback(async () => {
    if (pendingLink) {
      try {
        await window.electron.openExternal(pendingLink.href);
      } catch {
        await window.electron.showMessageBox({
          type: 'error',
          buttons: ['OK'],
          title: intl.formatMessage(i18n.failedToOpenLink),
          message: intl.formatMessage(i18n.noApplicationFound),
          detail: pendingLink.href,
        });
      }
    }
    setPendingLink(null);
  }, [pendingLink, intl]);

  const handleCancelOpen = useCallback(() => {
    setPendingLink(null);
  }, []);

  return (
    <>
      <div
        className={`w-full overflow-x-hidden prose prose-sm text-text-primary dark:prose-invert max-w-full word-break font-sans
        prose-pre:p-0 prose-pre:m-0 !p-0
        prose-code:break-all prose-code:whitespace-pre-wrap prose-code:font-mono
        prose-a:break-all prose-a:overflow-wrap-anywhere
        prose-table:table prose-table:w-full
        prose-blockquote:text-inherit
        prose-td:border prose-td:border-border-primary prose-td:p-2
        prose-th:border prose-th:border-border-primary prose-th:p-2
        prose-thead:bg-background-primary
        prose-h1:text-2xl prose-h1:font-normal prose-h1:mb-5 prose-h1:mt-0 prose-h1:font-sans
        prose-h2:text-xl prose-h2:font-normal prose-h2:mb-4 prose-h2:mt-4 prose-h2:font-sans
        prose-h3:text-lg prose-h3:font-normal prose-h3:mb-3 prose-h3:mt-3 prose-h3:font-sans
        prose-p:mt-0 prose-p:mb-2 prose-p:font-sans
        prose-ol:my-2 prose-ol:font-sans
        prose-ul:mt-0 prose-ul:mb-3 prose-ul:font-sans
        prose-li:m-0 prose-li:font-sans ${className}`}
      >
        <ReactMarkdown
          urlTransform={customUrlTransform}
          remarkPlugins={[remarkGfm, remarkBreaks, [remarkMath, { singleDollarTextMath: false }]]}
          rehypePlugins={[
            [
              rehypeKatex,
              {
                throwOnError: false,
                errorColor: '#cc0000',
                strict: false,
              },
            ],
          ]}
          components={{
            a: (props) => {
              return (
                <a
                  {...props}
                  target="_blank"
                  rel="noopener noreferrer"
                  onClick={(e) => {
                    e.preventDefault();
                    e.stopPropagation();
                    if (!props.href) return;

                    if (isProtocolSafe(props.href)) {
                      window.electron.openExternal(props.href);
                    } else {
                      const protocol = getProtocol(props.href);
                      if (!protocol) return;
                      setPendingLink({ protocol, href: props.href });
                    }
                  }}
                />
              );
            },
            code: MarkdownCode,
          }}
        >
          {processedContent}
        </ReactMarkdown>
      </div>
      <ConfirmationModal
        isOpen={pendingLink !== null}
        title={intl.formatMessage(i18n.openExternalLink)}
        message={intl.formatMessage(i18n.openProtocolLink, { protocol: pendingLink?.protocol ?? '' })}
        detail={intl.formatMessage(i18n.thisWillOpen, { href: pendingLink?.href ?? '' })}
        onConfirm={handleConfirmOpen}
        onCancel={handleCancelOpen}
        confirmLabel={intl.formatMessage(i18n.open)}
        cancelLabel={intl.formatMessage(i18n.cancel)}
      />
    </>
  );
});

export default MarkdownContent;
