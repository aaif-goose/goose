import { Loader2, X } from 'lucide-react';
import { defineMessages, useIntl } from '../i18n';
import type { ToolRequest, ToolResponse } from '../types/message';

const FINAL_OUTPUT_TOOL_NAME = 'recipe__final_output';

const i18n = defineMessages({
  returning: {
    id: 'finalOutputLine.returning',
    defaultMessage: 'Returning structured result…',
  },
  returned: {
    id: 'finalOutputLine.returned',
    defaultMessage: 'Returned structured result',
  },
  rejected: {
    id: 'finalOutputLine.rejected',
    defaultMessage: 'Structured result was rejected',
  },
});

export function isFinalOutputToolRequest(toolRequest: ToolRequest): boolean {
  const toolCall = toolRequest.toolCall as { status?: string; value?: { name?: string } };
  return toolCall.status === 'success' && toolCall.value?.name === FINAL_OUTPUT_TOOL_NAME;
}

interface FinalOutputLineProps {
  toolResponse?: ToolResponse;
  isStreaming: boolean;
}

export default function FinalOutputLine({ toolResponse, isStreaming }: FinalOutputLineProps) {
  const intl = useIntl();
  const toolResult = toolResponse?.toolResult as
    { status?: string; value?: { isError?: boolean } } | undefined;
  const isRejected = toolResult?.status === 'error' || toolResult?.value?.isError === true;
  const isPending = !toolResponse && isStreaming;

  const [Icon, label] = isPending
    ? [Loader2, i18n.returning]
    : isRejected
      ? [X, i18n.rejected]
      : [null, i18n.returned];

  return (
    <div className="flex items-center gap-1.5 text-sm text-text-secondary">
      {Icon && <Icon className={isPending ? 'h-3.5 w-3.5 animate-spin' : 'h-3.5 w-3.5'} />}
      <span>{intl.formatMessage(label)}</span>
    </div>
  );
}
