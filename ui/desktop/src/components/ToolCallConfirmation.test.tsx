import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../i18n/test-utils';
import type { ActionRequired } from '../types/message';
import ToolCallConfirmation from './ToolCallConfirmation';

vi.mock('./ToolApprovalButtons', () => ({
  default: ({ data }: { data: { generation?: string } }) => (
    <div data-testid="approval-buttons" data-generation={data.generation} />
  ),
}));

const securityPrompt = 'This command sends a local file to a remote service.';

const actionRequiredContent = {
  type: 'actionRequired',
  data: {
    actionType: 'toolConfirmation',
    generation: 'permission-generation-1',
    id: 'request-1',
    toolName: 'developer__shell',
    arguments: {
      command: 'upload /home/alice/private.txt to files.example.test',
    },
    prompt: securityPrompt,
  },
} as ActionRequired & { type: 'actionRequired' };

describe('ToolCallConfirmation', () => {
  it('shows the concrete tool arguments before approval', () => {
    render(
      <ToolCallConfirmation
        sessionId="session-1"
        isClicked={false}
        actionRequiredContent={actionRequiredContent}
      />,
      { wrapper: IntlTestWrapper }
    );

    expect(screen.getByText('command')).toBeInTheDocument();
    expect(screen.getByText(/upload \/home\/alice\/private\.txt/)).toBeInTheDocument();
    expect(screen.getByTestId('approval-buttons')).toHaveAttribute(
      'data-generation',
      'permission-generation-1'
    );
  });

  it('shows the security prompt before approval', () => {
    render(
      <ToolCallConfirmation
        sessionId="session-1"
        isClicked={false}
        actionRequiredContent={actionRequiredContent}
      />,
      { wrapper: IntlTestWrapper }
    );

    expect(screen.getByText(securityPrompt)).toBeInTheDocument();
    expect(screen.getByTestId('approval-buttons')).toBeInTheDocument();
  });
});
