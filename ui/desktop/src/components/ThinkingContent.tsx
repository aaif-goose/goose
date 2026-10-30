import { useState, useEffect } from 'react';
import MarkdownContent from './MarkdownContent';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from './ui/collapsible';
import Expand from './ui/Expand';
import { AppEvents } from '../constants/events';
import type { ShowThinking } from '../utils/settings';

interface ThinkingContentProps {
  content: string;
}

export default function ThinkingContent({ content }: ThinkingContentProps) {
  const [showThinking, setShowThinking] = useState<ShowThinking | null>(null);
  const [manualToggle, setManualToggle] = useState<boolean | null>(null);

  useEffect(() => {
    const loadShowThinking = () => {
      window.electron.getSetting('showThinking').then(setShowThinking);
    };

    loadShowThinking();
    window.addEventListener(AppEvents.SHOW_THINKING_CHANGED, loadShowThinking);

    return () => {
      window.removeEventListener(AppEvents.SHOW_THINKING_CHANGED, loadShowThinking);
    };
  }, []);

  // Wait for the setting so 'never' and 'always' do not flash a collapsed block.
  if (showThinking === null || showThinking === 'never') {
    return null;
  }

  const expanded = manualToggle !== null ? manualToggle : showThinking === 'always';

  return (
    <Collapsible open={expanded} onOpenChange={(open) => setManualToggle(open)} className="mb-2">
      <CollapsibleTrigger className="flex items-center gap-1.5 text-xs text-text-secondary hover:text-text-primary transition-colors cursor-pointer">
        <Expand size={3} isExpanded={expanded} />
        <span className="italic">Thinking</span>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="mt-1 ml-[18px] text-xs text-text-secondary italic">
          <MarkdownContent content={content} />
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}
