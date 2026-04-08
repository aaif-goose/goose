import { useState, useEffect, useRef } from 'react';
import MarkdownContent from './MarkdownContent';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from './ui/collapsible';
import Expand from './ui/Expand';

interface ThinkingContentProps {
  content: string;
  isExpanded: boolean;
}

export default function ThinkingContent({ content, isExpanded }: ThinkingContentProps) {
  const [manualToggle, setManualToggle] = useState<boolean | null>(null);
  const prevIsExpanded = useRef(isExpanded);

  useEffect(() => {
    if (prevIsExpanded.current && !isExpanded) {
      setManualToggle(null);
    }
    prevIsExpanded.current = isExpanded;
  }, [isExpanded]);

  const expanded = manualToggle !== null ? manualToggle : isExpanded;

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
