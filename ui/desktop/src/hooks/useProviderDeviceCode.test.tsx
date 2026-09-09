import { act, renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { useProviderDeviceCode } from './useProviderDeviceCode';

function dispatchDeviceCode(providerId: string, userCode: string) {
  window.dispatchEvent(
    new CustomEvent('goose:device-code', {
      detail: {
        providerId,
        userCode,
        verificationUri: 'https://example.com/device',
        expiresIn: 300,
      },
    })
  );
}

describe('useProviderDeviceCode', () => {
  it('only accepts device codes for the requested provider', () => {
    const { result } = renderHook(() => useProviderDeviceCode('github_copilot'));

    act(() => {
      dispatchDeviceCode('kimicode', 'KIMI-CODE');
    });
    expect(result.current.deviceCode).toBeNull();

    act(() => {
      dispatchDeviceCode('github_copilot', 'COPILOT-CODE');
    });
    expect(result.current.deviceCode?.userCode).toBe('COPILOT-CODE');
  });
});
