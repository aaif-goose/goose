import { useState } from 'react';
import { ChevronDown } from 'lucide-react';
import { Input } from '../../ui/input';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  conversationLimits: {
    id: 'conversationLimitsDropdown.conversationLimits',
    defaultMessage: 'Conversation Limits',
  },
  maxTurns: {
    id: 'conversationLimitsDropdown.maxTurns',
    defaultMessage: 'Max Turns',
  },
  maxTurnsDescription: {
    id: 'conversationLimitsDropdown.maxTurnsDescription',
    defaultMessage: 'Maximum agent turns before Goose asks for user input',
  },
});

interface ConversationLimitsDropdownProps {
  maxTurns: number;
  onMaxTurnsChange: (value: number) => void;
}

export const ConversationLimitsDropdown = ({
  maxTurns,
  onMaxTurnsChange,
}: ConversationLimitsDropdownProps) => {
  const intl = useIntl();
  const [isExpanded, setIsExpanded] = useState(false);

  const toggleExpanded = () => {
    setIsExpanded(!isExpanded);
  };

  return (
    <div className="pt-4">
      <button
        onClick={toggleExpanded}
        className="w-full flex items-center justify-between py-2 px-2 hover:bg-background-secondary rounded-lg transition-all group"
      >
        <h3 className="text-text-primary">{intl.formatMessage(i18n.conversationLimits)}</h3>

        <ChevronDown
          className={`w-4 h-4 text-text-secondary transition-transform duration-200 ease-in-out ${
            isExpanded ? 'rotate-180' : 'rotate-0'
          }`}
        />
      </button>

      <div
        className={`overflow-hidden transition-all duration-300 ease-in-out ${
          isExpanded ? 'max-h-96 opacity-100 mt-2' : 'max-h-0 opacity-0 mt-0'
        }`}
      >
        <div className="space-y-3 pb-2">
          <div className="flex items-center justify-between py-2 px-2 bg-background-secondary rounded-lg transform transition-all duration-200 ease-in-out">
            <div>
              <h4 className="text-text-primary text-sm">{intl.formatMessage(i18n.maxTurns)}</h4>
              <p className="text-xs text-text-secondary mt-[2px]">
                {intl.formatMessage(i18n.maxTurnsDescription)}
              </p>
            </div>
            <Input
              type="number"
              min="1"
              max="10000"
              value={maxTurns}
              onChange={(e) => onMaxTurnsChange(Number(e.target.value))}
              className="w-20"
            />
          </div>
        </div>
      </div>
    </div>
  );
};
