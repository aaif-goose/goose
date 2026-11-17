import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import GooseMessage from './GooseMessage';
import { AppEvents } from '../constants/events';
import { IntlTestWrapper } from '../i18n/test-utils';
import type { Message, MessageContent } from '../types/message';
import type { ShowThinking } from '../utils/settings';

const thought = 'Comparing the two options';
const answer = 'Use the second option';
const thinking: MessageContent = { type: 'thinking', thinking: thought, signature: '' };
const text: MessageContent = { type: 'text', text: answer };

function messageElement(content: MessageContent[], isStreaming: boolean) {
  const message: Message = {
    id: 'msg-1',
    role: 'assistant',
    created: 1_790_000_000,
    content,
    metadata: { agentVisible: true, userVisible: true },
  };
  return (
    <GooseMessage
      sessionId="session-1"
      message={message}
      hideTimestamp={false}
      toolStates={[]}
      toolNotifications={[]}
      toolConfirmationShownInline={false}
      append={() => {}}
      isStreaming={isStreaming}
    />
  );
}

function renderMessage(content: MessageContent[], isStreaming: boolean) {
  return render(messageElement(content, isStreaming), { wrapper: IntlTestWrapper });
}

function setShowThinking(value: ShowThinking) {
  return window.electron.setSetting('showThinking', value);
}

const findThinking = () => screen.findByRole('button', { name: 'Thinking' });
const queryThinking = () => screen.queryByRole('button', { name: 'Thinking' });

describe('GooseMessage thinking block', () => {
  afterEach(() => setShowThinking('collapsed'));

  it('stays collapsed while the model is still thinking', async () => {
    renderMessage([thinking], true);

    expect(await findThinking()).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByText(thought)).not.toBeInTheDocument();
  });

  it('keeps a block the user opened open after the answer starts', async () => {
    const user = userEvent.setup();
    const { rerender } = renderMessage([thinking], true);

    await user.click(await findThinking());
    expect(await findThinking()).toHaveAttribute('aria-expanded', 'true');

    rerender(messageElement([thinking, text], true));

    expect(await screen.findByText(answer)).toBeInTheDocument();
    expect(await findThinking()).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByText(thought)).toBeInTheDocument();
  });

  it('stays open after the answer starts when set to always', async () => {
    await setShowThinking('always');
    renderMessage([thinking, text], true);

    expect(await findThinking()).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByText(thought)).toBeInTheDocument();
  });

  it('is not shown when set to never', async () => {
    await setShowThinking('never');
    renderMessage([thinking, text], false);
    expect(queryThinking()).not.toBeInTheDocument();

    await act(async () => {});

    expect(await screen.findByText(answer)).toBeInTheDocument();
    expect(queryThinking()).not.toBeInTheDocument();
  });

  it('follows a setting change in an open chat', async () => {
    renderMessage([thinking, text], false);
    expect(await findThinking()).toBeInTheDocument();

    await setShowThinking('never');
    window.dispatchEvent(new CustomEvent(AppEvents.SHOW_THINKING_CHANGED));

    await waitFor(() => expect(queryThinking()).not.toBeInTheDocument());
  });
});
