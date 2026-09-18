import { afterEach, describe, expect, it, vi } from 'vitest';
import { JevDecisionModel } from './jev';
import { LlmDecisionModel } from './llm';
import type { DecisionModel } from './index';
import type { ChoiceQuestion } from './types';

const questions: Record<string, ChoiceQuestion> = {
  model: {
    instructions: 'Which model should handle this?',
    options: { 'fast-model': null, 'smart-model': 'For hard problems' },
  },
};

function mockFetch(payload: unknown, ok = true, status = 200) {
  const fetchMock = vi.fn().mockResolvedValue({
    ok,
    status,
    json: async () => payload,
    text: async () => JSON.stringify(payload),
    statusText: ok ? 'OK' : 'Error',
  });
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
}

function jevPayload(choice: string, confidence: number) {
  return { answers: { model: { type: 'choice', choice, confidence } } };
}

function llmPayload(content: string) {
  return { choices: [{ message: { content } }] };
}

afterEach(() => {
  vi.unstubAllGlobals();
});

/** Stub the main-process relay that the Jev adapter calls. */
function mockRelay(result: unknown) {
  const relay = vi.fn().mockResolvedValue(result);
  vi.stubGlobal('window', { electron: { decisionModelRequest: relay } });
  return relay;
}

describe('JevDecisionModel', () => {
  it('relays questions through main and returns typed answers', async () => {
    const relay = mockRelay({ ok: true, data: jevPayload('smart-model', 0.91) });

    const answers = await new JevDecisionModel().ask({ request: 'hi' }, questions);

    expect(answers.model).toEqual({ choice: 'smart-model', confidence: 0.91 });

    const request = relay.mock.calls[0][0];
    expect(request.url).toBe('https://api.typesafe.ai/v1/systemone');
    expect(request.apiKeyName).toBe('TYPESAFE_API_KEY');
    expect(request.body.model).toBe('jev-latest');
    expect(request.body.questions.model).toEqual({
      type: 'choice',
      instructions: 'Which model should handle this?',
      criteria: { 'fast-model': null, 'smart-model': 'For hard problems' },
    });
  });

  it('keeps the api key out of the renderer', async () => {
    const relay = mockRelay({ ok: true, data: jevPayload('fast-model', 0.9) });
    await new JevDecisionModel().ask({}, questions);
    expect(JSON.stringify(relay.mock.calls[0][0])).not.toContain('Bearer');
  });

  it('throws when the relay reports a failure', async () => {
    mockRelay({ ok: false, status: 429, error: 'rate limited' });
    await expect(new JevDecisionModel().ask({}, questions)).rejects.toThrow('429');
  });

  it('throws when an answer is missing', async () => {
    mockRelay({ ok: true, data: { answers: {} } });
    await expect(new JevDecisionModel().ask({}, questions)).rejects.toThrow('no answer');
  });
});

describe('LlmDecisionModel', () => {
  const config = { baseUrl: 'http://localhost:11434/v1', model: 'llama3' };

  it('parses JSON choices from a chat completion', async () => {
    const fetchMock = mockFetch(
      llmPayload('{"model": {"choice": "fast-model", "confidence": 0.8}}')
    );

    const answers = await new LlmDecisionModel(config).ask({ request: 'hi' }, questions);

    expect(answers.model).toEqual({ choice: 'fast-model', confidence: 0.8 });
    expect(fetchMock.mock.calls[0][0]).toBe('http://localhost:11434/v1/chat/completions');
  });

  it('tolerates JSON wrapped in prose or a code fence', async () => {
    mockFetch(
      llmPayload('Sure!\n```json\n{"model":{"choice":"fast-model","confidence":0.7}}\n```')
    );

    const answers = await new LlmDecisionModel(config).ask({}, questions);

    expect(answers.model.choice).toBe('fast-model');
  });

  it('rejects a choice that is not one of the options', async () => {
    mockFetch(llmPayload('{"model": {"choice": "hallucinated", "confidence": 0.9}}'));

    await expect(new LlmDecisionModel(config).ask({}, questions)).rejects.toThrow('invalid choice');
  });

  it('omits the auth header when no api key is configured', async () => {
    const fetchMock = mockFetch(llmPayload('{"model":{"choice":"fast-model","confidence":1}}'));

    await new LlmDecisionModel(config).ask({}, questions);

    const headers = (fetchMock.mock.calls[0][1] as { headers: Record<string, string> }).headers;
    expect(headers.Authorization).toBeUndefined();
  });
});

describe('contract parity', () => {
  it('both implementations return the same answer shape for the same question', async () => {
    mockRelay({ ok: true, data: jevPayload('fast-model', 0.8) });
    const jev: DecisionModel = new JevDecisionModel();
    const fromJev = await jev.ask({ request: 'hi' }, questions);

    mockFetch(llmPayload('{"model":{"choice":"fast-model","confidence":0.8}}'));
    const llm: DecisionModel = new LlmDecisionModel({ baseUrl: 'http://x/v1', model: 'm' });
    const fromLlm = await llm.ask({ request: 'hi' }, questions);

    expect(fromJev).toEqual(fromLlm);
  });
});
