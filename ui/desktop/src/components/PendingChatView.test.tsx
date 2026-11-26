/**
 * @vitest-environment jsdom
 */
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import PendingChatView from './PendingChatView';
import { IntlTestWrapper } from '../i18n/test-utils';

describe('PendingChatView', () => {
  it('shows the submitted message and a loading goose', () => {
    render(
      <IntlTestWrapper>
        <PendingChatView initialMessage={{ msg: 'hello from hub', images: [] }} />
      </IntlTestWrapper>
    );

    expect(screen.getByText('hello from hub')).toBeTruthy();
    expect(screen.getByTestId('loading-indicator')).toBeTruthy();
  });

  it('shows only the loading goose when there is no message yet', () => {
    render(
      <IntlTestWrapper>
        <PendingChatView />
      </IntlTestWrapper>
    );

    expect(screen.queryByText('hello from hub')).toBeNull();
    expect(screen.getByTestId('loading-indicator')).toBeTruthy();
  });
});
