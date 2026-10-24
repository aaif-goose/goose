import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { PluginErrorBoundary } from './PluginErrorBoundary';

function Crash(): never {
  throw new Error('plugin crashed');
}

describe('PluginErrorBoundary', () => {
  it('renders its children when nothing fails', () => {
    render(
      <PluginErrorBoundary resetKey="a" fallback={<p>fallback</p>}>
        <p>content</p>
      </PluginErrorBoundary>
    );

    expect(screen.getByText('content')).toBeInTheDocument();
    expect(screen.queryByText('fallback')).not.toBeInTheDocument();
  });

  it('shows the fallback when a child throws while rendering', () => {
    render(
      <PluginErrorBoundary resetKey="a" fallback={<p>fallback</p>}>
        <Crash />
      </PluginErrorBoundary>
    );

    expect(screen.getByText('fallback')).toBeInTheDocument();
  });

  it('recovers when the reset key changes', () => {
    const { rerender } = render(
      <PluginErrorBoundary resetKey="a" fallback={<p>fallback</p>}>
        <Crash />
      </PluginErrorBoundary>
    );

    rerender(
      <PluginErrorBoundary resetKey="b" fallback={<p>fallback</p>}>
        <p>fixed</p>
      </PluginErrorBoundary>
    );

    expect(screen.getByText('fixed')).toBeInTheDocument();
  });

  it('stays on the fallback while the reset key is unchanged', () => {
    const { rerender } = render(
      <PluginErrorBoundary resetKey="a" fallback={<p>fallback</p>}>
        <Crash />
      </PluginErrorBoundary>
    );

    rerender(
      <PluginErrorBoundary resetKey="a" fallback={<p>fallback</p>}>
        <p>fixed</p>
      </PluginErrorBoundary>
    );

    expect(screen.getByText('fallback')).toBeInTheDocument();
  });
});
