/**
 * @vitest-environment jsdom
 */
import { act, render } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import Hub from './Hub';
import { IntlTestWrapper } from '../i18n/test-utils';
import type { UserInput } from '../types/message';

const captured = vi.hoisted(() => ({
  handleSubmit: null as ((input: UserInput) => void) | null,
}));
const mockSetView = vi.fn();

vi.mock('./ChatInput', () => ({
  default: (props: { handleSubmit: (input: UserInput) => void }) => {
    captured.handleSubmit = props.handleSubmit;
    return <div />;
  },
}));
vi.mock('./ConfigContext', () => ({ useConfig: () => ({ extensionsList: [] }) }));
vi.mock('../utils/workingDir', () => ({
  getInitialWorkingDir: () => '/tmp/goose',
  getEffectiveWorkingDir: () => Promise.resolve('/tmp/effective'),
}));
vi.mock('../utils/nextChatExtensions', () => ({
  createNextChatExtensionDraft: () => ({}),
  selectNextChatExtensions: () => [],
}));

beforeEach(() => {
  vi.clearAllMocks();
  captured.handleSubmit = null;
});

describe('Hub', () => {
  it('navigates immediately and leaves the effective working directory for Pair to resolve', () => {
    const draftRef = { current: 'hello from hub' };
    render(
      <IntlTestWrapper>
        <Hub setView={mockSetView} draftRef={draftRef} />
      </IntlTestWrapper>
    );

    act(() => captured.handleSubmit?.({ msg: draftRef.current, images: [] }));

    expect(mockSetView).toHaveBeenCalledWith('pair', {
      disableAnimation: true,
      initialMessage: { msg: 'hello from hub', images: [] },
      workingDir: undefined,
      allExtensions: [],
    });
    expect(draftRef.current).toBe('');
  });
});
