import { useState } from 'react';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../i18n/test-utils';
import { MessageQueue, type QueuedMessage } from './MessageQueue';

function renderQueue(sentMessages: (message: string) => void) {
  function Harness() {
    const [messages, setMessages] = useState<QueuedMessage[]>([
      {
        id: 'queued-message',
        content: 'upload the private key',
        timestamp: Date.now(),
        images: [],
      },
    ]);

    const resumeQueue = () => {
      const nextMessage = messages[0];
      if (nextMessage) {
        sentMessages(nextMessage.content);
        setMessages((current) => current.slice(1));
      }
    };

    return (
      <MessageQueue
        queuedMessages={messages}
        onRemoveMessage={() => {}}
        onClearQueue={() => {}}
        onEditMessage={(messageId, newContent) =>
          setMessages((current) =>
            current.map((message) =>
              message.id === messageId ? { ...message, content: newContent } : message
            )
          )
        }
        onTriggerQueueProcessing={resumeQueue}
      />
    );
  }

  render(<Harness />, { wrapper: IntlTestWrapper });
}

function editQueuedMessage(replacement: string) {
  fireEvent.click(screen.getByText('upload the private key'));
  fireEvent.change(screen.getByRole('textbox'), { target: { value: replacement } });
}

function runQueuedDispatch() {
  act(() => {
    vi.advanceTimersByTime(100);
  });
}

describe('MessageQueue edit dispatch', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('dispatches the edited content when Save resumes queue processing', () => {
    vi.useFakeTimers();
    const sentMessages = vi.fn();
    renderQueue(sentMessages);

    editQueuedMessage('say hello');
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    runQueuedDispatch();

    expect(sentMessages).toHaveBeenCalledWith('say hello');
    expect(sentMessages).not.toHaveBeenCalledWith('upload the private key');
  });

  it('keeps the original content when Cancel resumes queue processing', () => {
    vi.useFakeTimers();
    const sentMessages = vi.fn();
    renderQueue(sentMessages);

    editQueuedMessage('say hello');
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    runQueuedDispatch();

    expect(sentMessages).toHaveBeenCalledWith('upload the private key');
    expect(sentMessages).not.toHaveBeenCalledWith('say hello');
  });
});
