/**
 * Just-in-time model routing: before a turn is sent, ask the configured decision
 * model which of the current provider's models should handle it and how much
 * thinking effort it needs.
 *
 * This feature is written against the DecisionModel contract, so it works the
 * same whether the decision is made by Jev or by a general-purpose LLM.
 */
import { acpListProviderModels } from '../acp/providers';
import type { ThinkingEffort } from '../types/providers';
import { getDecisionModel } from './config';
import type { ChoiceQuestion } from './types';

const MAX_CANDIDATE_MODELS = 12;

const THINKING_EFFORTS: Record<string, string> = {
  off: 'Trivial: greetings, acknowledgements, or a one-line factual answer',
  low: 'Simple: a small edit, a direct question, or a single lookup',
  medium: 'Moderate: multi-step work, debugging, or writing a contained feature',
  high: 'Hard: architecture, tricky bugs, or reasoning across many files',
};

export type RoutingDecision = {
  model: string;
  thinkingEffort: ThinkingEffort;
  /** Name of the decision model that made the call, for UI attribution. */
  decidedBy: string;
  /** Reported confidence in the model choice, 0 to 1. Surfaced, not gated on. */
  confidence: number;
  reason: string;
};

const log = (message: string, ...args: unknown[]) =>
  console.log(`%c[decisionModel] ${message}`, 'color:#7c3aed;font-weight:bold', ...args);

export function isModelRoutingEnabled(): boolean {
  const enabled = getDecisionModel() !== null;
  if (!enabled) {
    log('routing disabled: no decision model configured (set TYPESAFE_API_KEY)');
  }
  return enabled;
}

/**
 * Decide which model and thinking effort should handle this prompt.
 *
 * The decision is always applied when one comes back; confidence is surfaced in
 * the result for display rather than used as a gate. Returns null when routing is
 * not configured, the provider has fewer than two models to choose between, or
 * the decision model fails -- in those cases the caller keeps the current model.
 */
export async function decideModelForPrompt(
  prompt: string,
  provider: string
): Promise<RoutingDecision | null> {
  const decisionModel = getDecisionModel();
  if (!decisionModel || !prompt.trim()) return null;

  const startedAt = Date.now();
  try {
    const allModels = await acpListProviderModels(provider);
    const models = allModels.map((model) => model.id).slice(0, MAX_CANDIDATE_MODELS);
    log(`provider "${provider}" exposes ${allModels.length} model(s); considering`, models);
    if (models.length < 2) {
      log('skipping: need at least 2 candidate models to choose between');
      return null;
    }

    const questions: Record<string, ChoiceQuestion> = {
      model: {
        instructions:
          'Which model is the best fit to handle this coding-agent request? Prefer the cheapest or fastest model that can clearly do the job well.',
        options: Object.fromEntries(models.map((model) => [model, null])),
      },
      thinking_effort: {
        instructions: 'How much reasoning effort does this request require?',
        options: THINKING_EFFORTS,
      },
    };

    log(`asking ${decisionModel.name}...`);
    const answers = await decisionModel.ask({ request: prompt, provider }, questions);
    log(`${decisionModel.name} answered in ${Date.now() - startedAt}ms`, answers);

    const modelAnswer = answers.model;
    const effort = answers.thinking_effort.choice as ThinkingEffort;

    return {
      model: modelAnswer.choice,
      thinkingEffort: effort,
      decidedBy: decisionModel.name,
      confidence: modelAnswer.confidence,
      reason: `picked ${modelAnswer.choice} (${Math.round(modelAnswer.confidence * 100)}% confidence), ${effort} thinking`,
    };
  } catch (error) {
    console.error('[decisionModel] model routing failed', error);
    return null;
  }
}
