import { describe, expect, it } from 'vitest';
import { RequestError } from '@agentclientprotocol/sdk';
import {
  formatAcpError,
  parseAcpCreditsExhaustedError,
  parseAcpWorkingDirectoryMissingError,
} from '../errors';

describe('parseAcpWorkingDirectoryMissingError', () => {
  it.each(['/deleted/project', 'C:\\Users\\goose\\project', '\\\\server\\share\\project'])(
    'preserves the server path %s through SDK and wrapped errors',
    (path) => {
      const error = new RequestError(-32602, 'Invalid params', {
        reason: 'working_directory_missing',
        path,
      });
      const expected = { reason: 'working_directory_missing', path };
      expect(parseAcpWorkingDirectoryMissingError(error)).toEqual(expected);
      expect(parseAcpWorkingDirectoryMissingError({ error })).toEqual(expected);
    }
  );

  it.each([
    new Error('invalid directory path'),
    { message: 'Invalid params', data: { reason: 'working_directory_missing' } },
    { message: 'Invalid params', data: { reason: 'working_directory_missing', path: 42 } },
    { message: 'Invalid params', data: { reason: 'other', path: '/project' } },
  ])('ignores errors without a structured missing path', (error) => {
    expect(parseAcpWorkingDirectoryMissingError(error)).toBeNull();
  });
});

describe('formatAcpError', () => {
  it('explains how to recover from an authentication error', () => {
    expect(formatAcpError(RequestError.authRequired())).toBe(
      'Sign in to your provider, then try again.'
    );
  });
});

describe('parseAcpCreditsExhaustedError', () => {
  it('parses structured ACP credits exhausted errors', () => {
    expect(
      parseAcpCreditsExhaustedError({
        code: -32603,
        message: 'Please add credits to your account, then resend your message to continue.',
        data: {
          reason: 'credits_exhausted',
          url: 'https://router.tetrate.ai/billing',
        },
      })
    ).toEqual({
      message: 'Please add credits to your account, then resend your message to continue.',
      url: 'https://router.tetrate.ai/billing',
    });
  });

  it('parses wrapped JSON-RPC errors', () => {
    expect(
      parseAcpCreditsExhaustedError({
        error: {
          code: -32603,
          message: 'Add credits to continue.',
          data: {
            reason: 'credits_exhausted',
          },
        },
      })
    ).toEqual({
      message: 'Add credits to continue.',
    });
  });

  it('ignores non-credits-exhausted errors', () => {
    expect(
      parseAcpCreditsExhaustedError({
        code: -32603,
        message: 'Something failed.',
        data: {
          reason: 'provider_error',
        },
      })
    ).toBeNull();
  });
});
