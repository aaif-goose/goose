import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { IntlTestWrapper } from '../../i18n/test-utils';
import { ContextXrayDialog } from './ContextXrayDialog';
import type { ContextReport } from '../../acp/contextReport';

const getContextReport = vi.fn();

vi.mock('../../acp/contextReport', () => ({
  getContextReport: (...args: unknown[]) => getContextReport(...args),
}));

const report: ContextReport = {
  model: { provider: 'anthropic', modelName: 'claude-opus-5', contextLimit: 200_000 },
  totalTokens: 1_800,
  segments: [
    {
      category: 'system_prompt',
      label: 'Base system prompt',
      source: 'prompts/system.md',
      tokenCount: 1_000,
      contentPreview: 'You are goose',
      parts: [],
    },
    {
      category: 'tool_definitions',
      label: 'developer',
      source: null,
      tokenCount: 500,
      contentPreview: null,
      parts: [{ label: 'developer__shell', source: null, tokenCount: 500, contentPreview: null }],
    },
    {
      category: 'messages',
      label: 'User messages',
      source: null,
      tokenCount: 300,
      contentPreview: null,
      parts: [{ label: '#1', source: null, tokenCount: 300, contentPreview: 'hello' }],
    },
  ],
};

function renderDialog() {
  const props = {
    open: true,
    onOpenChange: vi.fn(),
    sessionId: 'session-1',
    totalTokens: 1_750,
    onCompact: vi.fn(),
    compactDisabled: false,
  };
  const view = render(
    <IntlTestWrapper>
      <ContextXrayDialog {...props} />
    </IntlTestWrapper>
  );
  const rerender = (next: Partial<typeof props>) =>
    view.rerender(
      <IntlTestWrapper>
        <ContextXrayDialog {...props} {...next} />
      </IntlTestWrapper>
    );
  return { props, rerender };
}

describe('ContextXrayDialog', () => {
  beforeEach(() => {
    getContextReport.mockReset();
  });

  it('renders the report and compacts from it', async () => {
    getContextReport.mockResolvedValue(report);
    const { props } = renderDialog();

    await waitFor(() => expect(screen.getByText('1.8k')).toBeInTheDocument());
    expect(getContextReport).toHaveBeenCalledWith('session-1');
    expect(screen.getByText('claude-opus-5 · 200k token context window')).toBeInTheDocument();
    expect(screen.getByText('System prompt')).toBeInTheDocument();
    expect(screen.getByText('Tool definitions')).toBeInTheDocument();
    expect(screen.getByText('Conversation')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Compact now' }));
    expect(props.onCompact).toHaveBeenCalledTimes(1);
    expect(props.onOpenChange).toHaveBeenCalledWith(false);
  });

  it('replaces a loaded report with an error and retry when a refresh fails', async () => {
    getContextReport
      .mockResolvedValueOnce(report)
      .mockRejectedValueOnce(new Error('boom'))
      .mockResolvedValueOnce(report);
    renderDialog();
    await waitFor(() => expect(screen.getByText('1.8k')).toBeInTheDocument());

    fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));

    await waitFor(() =>
      expect(screen.getByText('Could not load the context report.')).toBeInTheDocument()
    );
    expect(screen.queryByText('1.8k')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }));
    await waitFor(() => expect(screen.getByText('1.8k')).toBeInTheDocument());
  });

  it('drops the previous report when the session changes', async () => {
    getContextReport.mockResolvedValueOnce(report).mockReturnValueOnce(new Promise(() => {}));
    const { rerender } = renderDialog();
    await waitFor(() => expect(screen.getByText('1.8k')).toBeInTheDocument());

    rerender({ sessionId: 'session-2' });

    expect(getContextReport).toHaveBeenLastCalledWith('session-2');
    expect(screen.queryByText('1.8k')).not.toBeInTheDocument();
    expect(screen.getByTestId('context-report-loading')).toBeInTheDocument();
  });
});
