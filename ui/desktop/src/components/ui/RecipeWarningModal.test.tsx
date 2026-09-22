import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { Recipe } from '../../recipe';
import { IntlTestWrapper } from '../../i18n/test-utils';
import { RecipeWarningModal } from './RecipeWarningModal';

describe('RecipeWarningModal', () => {
  it('shows the complete recipe before consent and renders values as text', () => {
    const recipe: Recipe = {
      title: 'Untrusted recipe',
      description: '<img src=x onerror=alert(1)>',
      instructions: 'Run the workflow',
      retry: {
        max_retries: 3,
        checks: [{ type: 'shell', command: 'test -f /tmp/consent-check' }],
        on_failure: 'notify-send consent-failed',
      },
      extensions: [{ type: 'stdio', name: 'filesystem', cmd: 'filesystem-mcp' }],
    };

    render(<RecipeWarningModal isOpen recipe={recipe} onConfirm={vi.fn()} onCancel={vi.fn()} />, {
      wrapper: IntlTestWrapper,
    });

    const preview = screen.getByTestId('recipe-preview');
    expect(preview).toHaveTextContent('retry');
    expect(preview).toHaveTextContent('test -f /tmp/consent-check');
    expect(preview).toHaveTextContent('on_failure');
    expect(preview).toHaveTextContent('notify-send consent-failed');
    expect(preview).toHaveTextContent('extensions');
    expect(preview).toHaveTextContent('filesystem-mcp');
    expect(preview).toHaveTextContent('<img src=x onerror=alert(1)>');
    expect(preview.querySelector('img')).toBeNull();
  });
});
