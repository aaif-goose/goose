import { describe, expect, it } from 'vitest';
import { agentIdFromTunnelUrl } from './tunnel';

describe('agentIdFromTunnelUrl', () => {
  it('parses agent id from lapstone tunnel url', () => {
    expect(
      agentIdFromTunnelUrl('https://proxy.example/tunnel/abc123')
    ).toBe('abc123');
  });

  it('rejects urls without tunnel segment', () => {
    expect(agentIdFromTunnelUrl('https://proxy.example/nope')).toBeNull();
  });
});
