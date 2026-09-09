import { describe, expect, it } from 'vitest';
import { errorMessage } from './conversionUtils';

describe('errorMessage', () => {
  it('prefers ACP JSON-RPC error data over generic messages', () => {
    expect(
      errorMessage({
        error: {
          message: 'Invalid params',
          data: 'MLX backend error: failed to load model',
        },
      })
    ).toBe('MLX backend error: failed to load model');
  });

  it('prefers ACP JSON-RPC error data from Error instances', () => {
    const error = Object.assign(new Error('Invalid params'), {
      error: {
        message: 'Invalid params',
        data: 'MLX backend error: failed to load model',
      },
    });

    expect(errorMessage(error)).toBe('MLX backend error: failed to load model');
  });
});
