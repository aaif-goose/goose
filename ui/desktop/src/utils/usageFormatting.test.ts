import { describe, expect, it } from 'vitest';
import {
  formatCost,
  formatDuration,
  formatTokenCount,
  formatTokensPerSecond,
  tokensPerSecond,
} from './usageFormatting';

describe('usageFormatting', () => {
  describe('formatTokenCount', () => {
    it.each([
      [999, '999'],
      [1000, '1k'],
      [1234, '1.2k'],
      [12345, '12k'],
      [999_499, '999k'],
      [1_200_000, '1.2M'],
    ])('formats %d as %s', (input, expected) => {
      expect(formatTokenCount(input)).toBe(expected);
    });
  });

  describe('formatCost', () => {
    it.each([
      [0, '$0.00'],
      [1.236, '$1.24'],
      [0.0123, '$0.012'],
      [0.0004, '$0.0004'],
    ])('formats %d as %s', (input, expected) => {
      expect(formatCost(input)).toBe(expected);
    });
  });

  describe('formatDuration', () => {
    it.each([
      [840, '840ms'],
      [1000, '1s'],
      [4200, '4.2s'],
      [65_000, '1m 5s'],
    ])('formats %dms as %s', (input, expected) => {
      expect(formatDuration(input)).toBe(expected);
    });
  });

  describe('tokensPerSecond', () => {
    it('computes output tokens over elapsed seconds', () => {
      expect(tokensPerSecond(340, 4200)).toBeCloseTo(80.95, 2);
    });

    it.each([
      ['elapsedMs 0', 340, 0],
      ['elapsedMs null', 340, null],
      ['elapsedMs undefined', 340, undefined],
      ['outputTokens 0', 0, 4200],
      ['outputTokens null', null, 4200],
    ])('returns null for %s', (_name, outputTokens, elapsedMs) => {
      expect(tokensPerSecond(outputTokens, elapsedMs)).toBeNull();
    });
  });

  describe('formatTokensPerSecond', () => {
    it.each([
      [128.4, '128'],
      [48.23, '48.2'],
      [3.751, '3.75'],
      [100, '100'],
      [10, '10.0'],
    ])('formats %d as %s', (input, expected) => {
      expect(formatTokensPerSecond(input)).toBe(expected);
    });
  });
});
