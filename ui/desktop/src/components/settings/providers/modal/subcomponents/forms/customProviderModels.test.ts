import { describe, expect, it } from 'vitest';
import { resolveModelListUrls } from './CustomProviderForm';

describe('resolveModelListUrls', () => {
  it('does not double /v1 for OpenAI-compatible relays', () => {
    expect(resolveModelListUrls('https://relay.example.com/v1')).toEqual([
      'https://relay.example.com/v1/models',
    ]);
  });

  it('accepts a trailing slash on /v1', () => {
    expect(resolveModelListUrls('https://relay.example.com/v1/')).toEqual([
      'https://relay.example.com/v1/models',
    ]);
  });

  it('does not append /models twice', () => {
    expect(resolveModelListUrls('https://relay.example.com/v1/models')).toEqual([
      'https://relay.example.com/v1/models',
    ]);
  });

  it('falls back to /v1/models then /models when the host has no version', () => {
    expect(resolveModelListUrls('https://relay.example.com')).toEqual([
      'https://relay.example.com/v1/models',
      'https://relay.example.com/models',
    ]);
  });
});
