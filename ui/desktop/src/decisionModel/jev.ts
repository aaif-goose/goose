/**
 * Jev (TypeSafe System One) adapter.
 *
 * Jev is purpose-built for this: it evaluates every question in parallel against
 * the same state in a single call and returns calibrated probabilities, so its
 * answers map onto the DecisionModel contract directly with no parsing.
 *
 * The request is relayed through the main process because the API is
 * server-to-server and sends no CORS headers; that also keeps the API key out of
 * the renderer.
 *
 * Docs: https://docs.typesafe.ai/api
 */
import type { DecisionModel } from './index';
import type { ChoiceAnswer, ChoiceQuestion } from './types';

const API_URL = 'https://api.typesafe.ai/v1/systemone';
const MODEL = 'jev-latest';
export const JEV_API_KEY_NAME = 'TYPESAFE_API_KEY';

type JevAnswer = { choice?: string; confidence?: number };

export class JevDecisionModel implements DecisionModel {
  readonly name = 'Jev';

  async ask(
    state: unknown,
    questions: Record<string, ChoiceQuestion>
  ): Promise<Record<string, ChoiceAnswer>> {
    const body = {
      model: MODEL,
      state,
      questions: Object.fromEntries(
        Object.entries(questions).map(([key, question]) => [
          key,
          {
            type: 'choice',
            instructions: question.instructions,
            criteria: question.options,
          },
        ])
      ),
    };

    console.log('%c[jev] POST ' + API_URL, 'color:#7c3aed;font-weight:bold', body);

    const result = await window.electron.decisionModelRequest({
      url: API_URL,
      body,
      apiKeyName: JEV_API_KEY_NAME,
    });

    if (!result.ok) {
      console.error(`[jev] request failed (status ${result.status})`, result.error);
      throw new Error(`Jev request failed: ${result.status} ${result.error.slice(0, 200)}`);
    }

    const payload = result.data as {
      answers?: Record<string, JevAnswer | undefined>;
      usage?: unknown;
    };
    console.log('%c[jev] response', 'color:#7c3aed;font-weight:bold', payload);

    const answers = payload.answers ?? {};

    return Object.fromEntries(
      Object.keys(questions).map((key) => {
        const answer = answers[key];
        if (!answer?.choice) {
          throw new Error(`Jev returned no answer for "${key}"`);
        }
        return [key, { choice: answer.choice, confidence: answer.confidence ?? 0 }];
      })
    );
  }
}
