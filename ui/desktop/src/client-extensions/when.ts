import type { ExtensionHostContext, MessageExtensionHostContext } from './types';

function asMessageContext(context: ExtensionHostContext): MessageExtensionHostContext | null {
  if (!('role' in context) || typeof context.role !== 'string') {
    return null;
  }
  return context as MessageExtensionHostContext;
}

export function evaluateWhenClause(clause: string | undefined, context: ExtensionHostContext): boolean {
  if (!clause || !clause.trim()) {
    return true;
  }

  const trimmed = clause.trim();

  if (trimmed === 'session.active') {
    return context.sessionId !== null && context.sessionId.length > 0;
  }

  if (trimmed === '!session.active') {
    return context.sessionId === null || context.sessionId.length === 0;
  }

  if (trimmed === 'route.pair') {
    return context.route === '/pair';
  }

  const messageContext = asMessageContext(context);
  if (!messageContext) {
    return false;
  }

  if (trimmed === 'message.hasText') {
    return messageContext.hasText;
  }

  if (trimmed === 'message.hasImage') {
    return messageContext.hasImage;
  }

  if (trimmed === 'message.hasToolRequests') {
    return messageContext.hasToolRequests;
  }

  if (trimmed === 'message.role.user') {
    return messageContext.role === 'user';
  }

  if (trimmed === 'message.role.assistant') {
    return messageContext.role === 'assistant';
  }

  if (trimmed.startsWith('message.codeLanguage.')) {
    const language = trimmed.slice('message.codeLanguage.'.length).toLowerCase();
    return messageContext.codeLanguages.includes(language);
  }

  return false;
}
