import { defineMessages } from '../../i18n';
import type { ContextCategory } from '../../acp/contextReport';

export const categoryMessages = defineMessages({
  system_prompt: {
    id: 'contextXray.category.systemPrompt',
    defaultMessage: 'System prompt',
  },
  extension_instructions: {
    id: 'contextXray.category.extensionInstructions',
    defaultMessage: 'Extension instructions',
  },
  additional_instructions: {
    id: 'contextXray.category.additionalInstructions',
    defaultMessage: 'Instructions & hints',
  },
  turn_context: {
    id: 'contextXray.category.turnContext',
    defaultMessage: 'Turn context',
  },
  tool_definitions: {
    id: 'contextXray.category.toolDefinitions',
    defaultMessage: 'Tool definitions',
  },
  compaction_summary: {
    id: 'contextXray.category.compactionSummary',
    defaultMessage: 'Compaction summary',
  },
  messages: {
    id: 'contextXray.category.messages',
    defaultMessage: 'Conversation',
  },
});

export const categoryColorClass: Record<ContextCategory, string> = {
  system_prompt: 'bg-[#eda100] dark:bg-[#c98500]',
  extension_instructions: 'bg-[#008300]',
  additional_instructions: 'bg-[#4a3aa7] dark:bg-[#9085e9]',
  turn_context: 'bg-[#e34948] dark:bg-[#e66767]',
  tool_definitions: 'bg-[#1baf7a] dark:bg-[#199e70]',
  compaction_summary: 'bg-[#b5359b] dark:bg-[#d76bc4]',
  messages: 'bg-[#2a78d6] dark:bg-[#3987e5]',
};
