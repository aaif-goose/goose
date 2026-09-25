import { describe, expect, it } from 'vitest';
import { parseExtensionToHostMessage } from './messages';

describe('parseExtensionToHostMessage', () => {
  it('accepts every message an extension may send', () => {
    expect(parseExtensionToHostMessage({ type: 'grc/ui/showMessage', text: 'hi' })).toEqual({
      type: 'grc/ui/showMessage',
      text: 'hi',
    });
    expect(parseExtensionToHostMessage({ type: 'grc/chat/setInput', text: 'draft' })).toEqual({
      type: 'grc/chat/setInput',
      text: 'draft',
    });
    expect(parseExtensionToHostMessage({ type: 'grc/resize', height: 120 })).toEqual({
      type: 'grc/resize',
      height: 120,
    });
  });

  it('accepts host invocations with and without a call id or payload', () => {
    expect(
      parseExtensionToHostMessage({ type: 'grc/host/invoke', capability: 'x', method: 'run' })
    ).toEqual({ type: 'grc/host/invoke', capability: 'x', method: 'run' });
    expect(
      parseExtensionToHostMessage({
        type: 'grc/host/invoke',
        capability: 'x',
        method: 'run',
        id: 'call-1',
        payload: { a: 1 },
      })
    ).toEqual({
      type: 'grc/host/invoke',
      capability: 'x',
      method: 'run',
      id: 'call-1',
      payload: { a: 1 },
    });
  });

  it('rejects fields of the wrong type instead of passing them to the host', () => {
    expect(parseExtensionToHostMessage({ type: 'grc/chat/setInput', text: { a: 1 } })).toBeNull();
    expect(parseExtensionToHostMessage({ type: 'grc/ui/showMessage', text: 42 })).toBeNull();
    expect(parseExtensionToHostMessage({ type: 'grc/resize', height: '100' })).toBeNull();
    expect(parseExtensionToHostMessage({ type: 'grc/resize', height: Number.NaN })).toBeNull();
    expect(
      parseExtensionToHostMessage({
        type: 'grc/host/invoke',
        capability: 'x',
        method: 'run',
        id: 7,
      })
    ).toBeNull();
  });

  it('rejects unknown types and non-object values', () => {
    expect(parseExtensionToHostMessage({ type: 'grc/mesh/check' })).toBeNull();
    expect(parseExtensionToHostMessage({ text: 'no type' })).toBeNull();
    expect(parseExtensionToHostMessage(null)).toBeNull();
    expect(parseExtensionToHostMessage('grc/resize')).toBeNull();
    expect(parseExtensionToHostMessage([])).toBeNull();
  });
});
