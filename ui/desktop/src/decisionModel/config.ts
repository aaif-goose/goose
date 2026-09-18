/**
 * Selects which DecisionModel implementation to use.
 *
 * A native decision model wins when configured; otherwise any OpenAI-compatible
 * endpoint can serve the same features via the LLM adapter. Adding another
 * System One style model later means adding a case here and an adapter file --
 * features stay untouched.
 */
import type { DecisionModel } from './index';
import { JevDecisionModel } from './jev';
import { LlmDecisionModel } from './llm';

function config(key: string): string | undefined {
  const value = window.appConfig?.get(key);
  return typeof value === 'string' && value.trim() ? value.trim() : undefined;
}

let cached: DecisionModel | null | undefined;

function create(): DecisionModel | null {
  const jevApiKey = config('TYPESAFE_API_KEY');
  if (jevApiKey) {
    console.log(
      `%c[decisionModel] using Jev (key ending ...${jevApiKey.slice(-4)})`,
      'color:#7c3aed;font-weight:bold'
    );
    return new JevDecisionModel();
  }

  const baseUrl = config('DECISION_MODEL_BASE_URL');
  const model = config('DECISION_MODEL');
  if (baseUrl && model) {
    console.log(
      `%c[decisionModel] using LLM adapter: ${model} at ${baseUrl}`,
      'color:#7c3aed;font-weight:bold'
    );
    return new LlmDecisionModel({ baseUrl, model, apiKey: config('DECISION_MODEL_API_KEY') });
  }

  console.log(
    '%c[decisionModel] no decision model configured. TYPESAFE_API_KEY=%s',
    'color:#7c3aed;font-weight:bold',
    window.appConfig?.get('TYPESAFE_API_KEY') === undefined ? '<missing from appConfig>' : '<empty>'
  );
  return null;
}

/** The configured decision model, or null when decision features are disabled. */
export function getDecisionModel(): DecisionModel | null {
  if (cached === undefined) {
    cached = create();
  }
  return cached;
}

export function resetDecisionModelForTests(): void {
  cached = undefined;
}
