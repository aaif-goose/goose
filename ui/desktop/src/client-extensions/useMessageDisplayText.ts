import { useMemo } from 'react';
import { useLocation } from 'react-router';
import type { Message } from '../types/message';
import { useClientExtensions } from './ClientExtensionsContext';
import {
  buildMessageExtensionContext,
  extractCodeBlocks,
  stripFirstCodeBlockForLanguage,
} from './messageContext';

export function useMessageDisplayText(
  sessionId: string,
  message: Message,
  displayText: string,
  imageCount: number
): string {
  const location = useLocation();
  const { getCustomRender, loading } = useClientExtensions();

  return useMemo(() => {
    if (loading || !displayText.trim()) {
      return displayText;
    }

    const messageContext = buildMessageExtensionContext(
      sessionId,
      location.pathname,
      message,
      displayText,
      imageCount
    );
    const codeBlocks = extractCodeBlocks(displayText);
    const customRender = getCustomRender(messageContext, codeBlocks);
    const matchedLanguage = customRender?.match.language;
    if (!matchedLanguage) {
      return displayText;
    }

    const hasMatchingBlock = codeBlocks.some((block) => block.language === matchedLanguage);
    if (!hasMatchingBlock) {
      return displayText;
    }

    return stripFirstCodeBlockForLanguage(displayText, matchedLanguage);
  }, [
    displayText,
    getCustomRender,
    imageCount,
    loading,
    location.pathname,
    message,
    sessionId,
  ]);
}
