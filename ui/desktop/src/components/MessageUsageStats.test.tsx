import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import type { MessageUsage } from '../types/message';
import { IntlTestWrapper } from '../i18n/test-utils';
import MessageUsageStats from './MessageUsageStats';

// Radix Tooltip positioning (floating-ui) needs ResizeObserver, which jsdom lacks.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}

beforeAll(() => {
  vi.stubGlobal('ResizeObserver', ResizeObserverStub);
});

afterAll(() => {
  vi.unstubAllGlobals();
});

const FULL_USAGE: MessageUsage = {
  inputTokens: 1200,
  outputTokens: 340,
  totalTokens: 1540,
  cacheReadTokens: 800,
  cacheWriteTokens: 100,
  cost: 0.0123,
  costSource: 'estimated',
  elapsedMs: 4200,
  timeToFirstTokenMs: 840,
  isCompaction: false,
};

function renderUsage(usage: MessageUsage) {
  return render(<MessageUsageStats usage={usage} />, { wrapper: IntlTestWrapper });
}

describe('MessageUsageStats', () => {
  it('shows speed, estimated cost, and total tokens in the chip', () => {
    renderUsage(FULL_USAGE);

    // 340 tokens over 4.2s -> 80.95 tok/s, one decimal at this magnitude.
    expect(screen.getByText('81.0 tok/s')).toBeInTheDocument();
    expect(screen.getByText('~$0.012')).toBeInTheDocument();
    expect(screen.getByText('1.5k tok')).toBeInTheDocument();
  });

  it('drops the ~ prefix for provider-reported cost', () => {
    renderUsage({ ...FULL_USAGE, costSource: 'provider_reported' });

    expect(screen.getByText('$0.012')).toBeInTheDocument();
    expect(screen.queryByText('~$0.012')).not.toBeInTheDocument();
  });

  it('shows only the token segment when cost and timing are missing', () => {
    const { container } = renderUsage({ totalTokens: 1540 });

    expect(container).toHaveTextContent('1.5k tok');
    expect(container.textContent).not.toContain('tok/s');
    expect(container.textContent).not.toContain('$');
    expect(container.textContent).not.toContain('·');
  });

  it('renders nothing when the usage carries no displayable data', () => {
    const { container } = renderUsage({});
    expect(container).toBeEmptyDOMElement();

    const { container: nullContainer } = renderUsage({
      inputTokens: null,
      outputTokens: null,
      totalTokens: null,
      cost: null,
      elapsedMs: null,
    });
    expect(nullContainer).toBeEmptyDOMElement();
  });

  it('breaks down tokens and cost in the tooltip on hover', async () => {
    const user = userEvent.setup();
    renderUsage(FULL_USAGE);

    await user.hover(screen.getByText('1.5k tok'));

    // Radix mirrors the tooltip children into the role="tooltip" node; scoping
    // there keeps queries unique (the visible popper duplicates the text).
    const tooltip = within(await screen.findByRole('tooltip'));

    expect(tooltip.getByText('Input')).toBeInTheDocument();
    expect(tooltip.getByText('1.2k')).toBeInTheDocument();
    expect(tooltip.getByText('Output')).toBeInTheDocument();
    expect(tooltip.getByText('340')).toBeInTheDocument();
    expect(tooltip.getByText('(estimated)')).toBeInTheDocument();
  });
});
