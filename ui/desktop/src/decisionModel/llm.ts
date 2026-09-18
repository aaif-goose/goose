/**
 * General-purpose LLM adapter, for any OpenAI-compatible /chat/completions
 * endpoint (OpenAI, Ollama, llama.cpp, vLLM, ...).
 *
 * This is the escape hatch from proprietary decision models: it lets the same
 * features run against a local or self-hosted model. It is less efficient than a
 * purpose-built System One model -- one prompt carries every question, so the
 * questions are not isolated from each other -- and self-reported confidence is
 * not calibrated the way a probability distribution is. Prefer a native decision
 * model when one is configured.
 */
import type { DecisionModel } from './index';
import type { ChoiceAnswer, ChoiceQuestion } from './types';

const SYSTEM_PROMPT = `You are a decision engine. Evaluate each question against the state and pick the single best option.
Reply with JSON only, in the shape {"<question id>": {"choice": "<option>", "confidence": <0..1>}}.
Use exactly the given question ids and pick "choice" verbatim from that question's options.
"confidence" is how sure you are: use a low value when the state is ambiguous or you are guessing.`;

export type LlmDecisionModelConfig = {
  baseUrl: string;
  model: string;
  apiKey?: string;
};

function buildUserPrompt(state: unknown, questions: Record<string, ChoiceQuestion>): string {
  const rendered = Object.entries(questions).map(([id, question]) => {
    const options = Object.entries(question.options).map(([option, rubric]) =>
      rubric ? `  - ${option}: ${rubric}` : `  - ${option}`
    );
    return `Question "${id}": ${question.instructions}\nOptions:\n${options.join('\n')}`;
  });

  return `State:\n${JSON.stringify(state, null, 2)}\n\n${rendered.join('\n\n')}`;
}

/** Tolerate models that wrap JSON in prose or a ```json fence. */
function parseJsonObject(content: string): Record<string, unknown> {
  const start = content.indexOf('{');
  const end = content.lastIndexOf('}');
  if (start === -1 || end <= start) {
    throw new Error('Decision model did not return JSON');
  }
  return JSON.parse(content.slice(start, end + 1));
}

export class LlmDecisionModel implements DecisionModel {
  readonly name: string;

  constructor(private readonly config: LlmDecisionModelConfig) {
    this.name = config.model;
  }

  async ask(
    state: unknown,
    questions: Record<string, ChoiceQuestion>
  ): Promise<Record<string, ChoiceAnswer>> {
    const response = await fetch(`${this.config.baseUrl.replace(/\/$/, '')}/chat/completions`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.config.apiKey ? { Authorization: `Bearer ${this.config.apiKey}` } : {}),
      },
      body: JSON.stringify({
        model: this.config.model,
        temperature: 0,
        response_format: { type: 'json_object' },
        messages: [
          { role: 'system', content: SYSTEM_PROMPT },
          { role: 'user', content: buildUserPrompt(state, questions) },
        ],
      }),
    });

    if (!response.ok) {
      throw new Error(`Decision model request failed: ${response.status}`);
    }

    const payload = (await response.json()) as {
      choices?: { message?: { content?: string } }[];
    };
    const content = payload.choices?.[0]?.message?.content;
    if (!content) {
      throw new Error('Decision model returned an empty response');
    }

    const parsed = parseJsonObject(content);

    return Object.fromEntries(
      Object.entries(questions).map(([key, question]) => {
        const answer = parsed[key] as { choice?: unknown; confidence?: unknown } | undefined;
        const choice = answer?.choice;
        if (typeof choice !== 'string' || !(choice in question.options)) {
          throw new Error(`Decision model returned an invalid choice for "${key}"`);
        }
        const confidence = typeof answer?.confidence === 'number' ? answer.confidence : 0;
        return [key, { choice, confidence: Math.min(Math.max(confidence, 0), 1) }];
      })
    );
  }
}
