import type { Message } from '../types/message';
import type { MessageExtensionHostContext } from './types';
import { getToolRequests } from '../types/message';

const CODE_FENCE_PATTERN = /```(\w+)?[^\n]*\n([\s\S]*?)```/g;

export function extractCodeBlocks(text: string): { language: string; content: string }[] {
  const blocks: { language: string; content: string }[] = [];
  const pattern = new RegExp(CODE_FENCE_PATTERN.source, 'g');

  for (const match of text.matchAll(pattern)) {
    const language = (match[1] ?? '').trim().toLowerCase();
    const content = match[2]?.trim() ?? '';
    if (language || content) {
      blocks.push({ language, content });
    }
  }

  return blocks;
}

export function extractCodeLanguages(text: string): string[] {
  const languages = new Set<string>();
  for (const block of extractCodeBlocks(text)) {
    if (block.language) {
      languages.add(block.language);
    }
  }
  return [...languages];
}

export function stripCodeBlocksForLanguage(text: string, language: string): string {
  const pattern = new RegExp(
    `\`\`\`${language.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}[^\\n]*\\n[\\s\\S]*?\`\`\``,
    'gi'
  );
  return text.replace(pattern, '').replace(/\n{3,}/g, '\n\n').trim();
}

export function stripFirstCodeBlockForLanguage(text: string, language: string): string {
  const pattern = new RegExp(
    `\`\`\`${language.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}[^\\n]*\\n[\\s\\S]*?\`\`\``,
    'i'
  );
  return text.replace(pattern, '').replace(/\n{3,}/g, '\n\n').trim();
}

export function buildMessageExtensionContext(
  sessionId: string | null,
  route: string,
  message: Message,
  displayText: string,
  imageCount: number
): MessageExtensionHostContext {
  const toolRequests = getToolRequests(message);

  return {
    sessionId,
    route,
    messageId: message.id ?? null,
    role: message.role,
    hasText: displayText.trim().length > 0,
    hasImage: imageCount > 0,
    hasToolRequests: toolRequests.length > 0,
    codeLanguages: extractCodeLanguages(displayText),
  };
}
