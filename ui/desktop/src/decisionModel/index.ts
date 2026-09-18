/**
 * DecisionModel: a provider-agnostic contract for fast, structured decisions.
 *
 * Features ask a decision model typed questions and branch on the typed answers,
 * rather than parsing free text. The contract is deliberately narrow so it can be
 * served either by a purpose-built System One model (see ./jev.ts) or by any
 * general-purpose LLM asked to emit JSON (see ./llm.ts).
 *
 * Only the `choice` question type is modeled today because that is all the
 * current features need. Score and noul questions can be added when a feature
 * needs them.
 */
import type { ChoiceQuestion, ChoiceAnswer } from './types';

export type { ChoiceQuestion, ChoiceAnswer } from './types';

export interface DecisionModel {
  /** Short label for logs and UI attribution, e.g. "Jev". */
  readonly name: string;

  /**
   * Evaluate questions against a shared state, returning one answer per key.
   *
   * Implementations answer every question or throw; callers gate on each
   * answer's confidence rather than expecting partial results.
   */
  ask(
    state: unknown,
    questions: Record<string, ChoiceQuestion>
  ): Promise<Record<string, ChoiceAnswer>>;
}
