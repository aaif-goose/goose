import { describe, expect, it } from 'vitest';
import defaultFixture from '../../fixtures/routing_prefs_default.json';
import { DEFAULT_ROUTING_PREFS, parseRoutingPrefs } from './prefs';

describe('routing prefs schema sync', () => {
  it('default matches Rust fixture', () => {
    expect(DEFAULT_ROUTING_PREFS).toEqual(defaultFixture);
  });

  it('parseRoutingPrefs accepts fixture', () => {
    expect(parseRoutingPrefs(defaultFixture)).toEqual(defaultFixture);
  });
});
