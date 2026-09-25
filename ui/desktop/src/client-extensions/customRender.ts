import type {
  CodeBlock,
  CustomRenderMatch,
  MessageExtensionHostContext,
  RegisteredCustomRender,
} from './types';
import { evaluateWhenClause } from './when';

export function matchesCustomRender(
  match: CustomRenderMatch,
  context: MessageExtensionHostContext,
  codeBlocks: CodeBlock[]
): boolean {
  if (match.contentType === 'text') {
    return context.hasText && codeBlocks.length === 0;
  }

  if (match.contentType === 'code' || match.language) {
    if (codeBlocks.length === 0) {
      return false;
    }
  }

  if (match.language) {
    return context.codeLanguages.includes(match.language.toLowerCase());
  }

  if (match.contentType === 'code') {
    return codeBlocks.length > 0;
  }

  return true;
}

export function selectCustomRender(
  renders: RegisteredCustomRender[],
  context: MessageExtensionHostContext,
  codeBlocks: CodeBlock[]
): RegisteredCustomRender | null {
  const candidates = renders.filter(
    (render) =>
      evaluateWhenClause(render.when, context) &&
      matchesCustomRender(render.match, context, codeBlocks)
  );

  if (candidates.length === 0) {
    return null;
  }

  return candidates.reduce((best, current) =>
    (current.priority ?? 0) > (best.priority ?? 0) ? current : best
  );
}
