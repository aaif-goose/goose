import { describe, expect, it } from 'vitest';
import { hasConfiguredSecret, isMaskedConfigValue } from '../config';

describe('ACP config values', () => {
  it('recognizes masked secret values', () => {
    const value = { maskedValue: '********' };

    expect(isMaskedConfigValue(value)).toBe(true);
    expect(hasConfiguredSecret(value)).toBe(true);
  });

  it.each([null, '', '********', {}, { maskedValue: '' }, { maskedValue: 1 }])(
    'does not treat %j as a configured secret',
    (value) => {
      expect(hasConfiguredSecret(value)).toBe(false);
    }
  );
});
