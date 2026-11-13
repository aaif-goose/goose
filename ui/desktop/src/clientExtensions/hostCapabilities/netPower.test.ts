import { afterEach, describe, expect, it, vi } from 'vitest';
import { createHostSession } from './session';

const acpMocks = vi.hoisted(() => ({
  acpFetchClientExtensionNet: vi.fn(),
}));

vi.mock('../../acp/clientExtensions', () => ({
  acpFetchClientExtensionNet: acpMocks.acpFetchClientExtensionNet,
}));

const actions = {
  startChat: vi.fn(),
  createSession: vi.fn(),
  openSession: vi.fn(),
  openPage: vi.fn(),
};

async function invoke(allowedOrigins: string[], payload?: unknown, extensionId = 'demo') {
  const post = vi.fn();
  await createHostSession(extensionId, ['net:fetch'], post, actions, allowedOrigins).handleInvoke({
    type: 'grc/host/invoke',
    capability: 'net',
    method: 'fetch',
    payload,
  });
  return post.mock.calls[0][0];
}

afterEach(() => {
  vi.clearAllMocks();
});

describe('net power', () => {
  it('rejects a URL whose origin is not in the manifest allowlist', async () => {
    const result = await invoke([], { url: 'http://127.0.0.1:8455/api/repos' });

    expect(result.error).toContain('has not allow-listed origin "http://127.0.0.1:8455"');
  });

  it('rejects an invalid URL before checking the allowlist', async () => {
    const result = await invoke(['http://127.0.0.1:8455'], { url: 'not a url' });

    expect(result.error).toBe('Invalid URL "not a url"');
  });

  it('calls the ACP backend only for an allow-listed origin, scoped to the caller', async () => {
    acpMocks.acpFetchClientExtensionNet.mockResolvedValue({
      ok: true,
      status: 200,
      headers: {},
      text: '{}',
    });

    const result = await invoke(
      ['http://127.0.0.1:8455'],
      {
        url: 'http://127.0.0.1:8455/api/repos',
        headers: { 'X-Loupe-Capability': 'tok' },
      },
      'demo'
    );

    expect(acpMocks.acpFetchClientExtensionNet).toHaveBeenCalledWith(
      'demo',
      'http://127.0.0.1:8455/api/repos',
      'GET',
      { 'X-Loupe-Capability': 'tok' },
      undefined
    );
    expect(result.payload).toEqual({ ok: true, status: 200, headers: {}, text: '{}' });
  });

  it('rejects a different origin under the same allow-listed host', async () => {
    const result = await invoke(['http://127.0.0.1:8455'], {
      url: 'http://127.0.0.1:9999/api/repos',
    });

    expect(result.error).toContain('"http://127.0.0.1:9999"');
    expect(acpMocks.acpFetchClientExtensionNet).not.toHaveBeenCalled();
  });

  it('surfaces the backend error when the response exceeds the size limit', async () => {
    acpMocks.acpFetchClientExtensionNet.mockRejectedValue(
      new Error('Response exceeded 5242880 bytes')
    );

    const result = await invoke(['http://127.0.0.1:8455'], {
      url: 'http://127.0.0.1:8455/api/repos',
    });

    expect(result.error).toContain('exceeded 5242880 bytes');
  });

  it('rejects an unsupported method and an oversized body', async () => {
    const badMethod = await invoke(['http://127.0.0.1:8455'], {
      url: 'http://127.0.0.1:8455/api/repos',
      method: 'TRACE',
    });
    const oversized = await invoke(['http://127.0.0.1:8455'], {
      url: 'http://127.0.0.1:8455/api/repos',
      method: 'POST',
      body: 'x'.repeat(512 * 1024 + 1),
    });

    expect(badMethod.error).toContain('Invalid "method"');
    expect(oversized.error).toContain('Invalid "body"');
  });

  it('requires net:fetch', async () => {
    const post = vi.fn();
    await createHostSession('demo', [], post, actions, ['http://127.0.0.1:8455']).handleInvoke({
      type: 'grc/host/invoke',
      capability: 'net',
      method: 'fetch',
      payload: { url: 'http://127.0.0.1:8455/api/repos' },
    });

    expect(post.mock.calls[0][0].error).toContain('net:fetch');
  });
});
