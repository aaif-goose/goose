import React, { useState, useRef, useEffect } from 'react';
import { Plus } from 'lucide-react';
import { toast } from 'react-toastify';
import TagPill from './TagPill';
import { addTagsToSession, removeTagFromSession, SessionTag } from '../../utils/tags';

const PREDEFINED_TAGS = [
  'python', 'javascript', 'typescript', 'rust', 'html-css',
  'frontend', 'backend', 'fullstack', 'api', 'database',
  'debugging', 'refactoring', 'testing', 'devops', 'deployment',
  'data-analysis', 'machine-learning', 'automation', 'scripting',
  'documentation', 'code-review', 'git', 'setup', 'configuration',
  'web-scraping', 'game-dev', 'cli-tool', 'file-management',
  'research', 'writing', 'general',
];

interface TagInputProps {
  sessionId: string;
  tags: SessionTag[];
  onTagAdded: (tag: string) => void;
  onTagRemoved: (tag: string) => void;
}

const TagInput: React.FC<TagInputProps> = ({ sessionId, tags, onTagAdded, onTagRemoved }) => {
  const [inputValue, setInputValue]     = useState('');
  const [showDropdown, setShowDropdown] = useState(false);
  const [isAdding, setIsAdding]         = useState(false);
  const inputRef  = useRef<HTMLInputElement>(null);
  const wrapperRef = useRef<HTMLDivElement>(null);

  const existingTagNames = tags.map((t) => t.tag);

  const suggestions = PREDEFINED_TAGS.filter(
    (t) => t.includes(inputValue.toLowerCase().trim()) && !existingTagNames.includes(t),
  ).slice(0, 6);

  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (wrapperRef.current && !wrapperRef.current.contains(e.target as Node)) {
        setShowDropdown(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  const addTag = async (tag: string) => {
    const normalized = tag.toLowerCase().trim();
    if (!normalized || existingTagNames.includes(normalized) || isAdding) return;

    setIsAdding(true);
    try {
      await addTagsToSession(sessionId, [normalized], 'manual');
      onTagAdded(normalized);
      setInputValue('');
      setShowDropdown(false);
    } catch {
      toast.error(`Failed to add tag "${normalized}"`);
    } finally {
      setIsAdding(false);
    }
  };

  const removeTag = async (tag: string) => {
    try {
      await removeTagFromSession(sessionId, tag);
      onTagRemoved(tag);
    } catch {
      toast.error(`Failed to remove tag "${tag}"`);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      addTag(inputValue);
    } else if (e.key === 'Escape') {
      setShowDropdown(false);
      setInputValue('');
    }
  };

  return (
    <div className="flex flex-wrap items-center gap-1" ref={wrapperRef}>
      {tags.map((t) => (
        <TagPill
          key={t.tag}
          tag={t.tag}
          size="sm"
          onRemove={() => removeTag(t.tag)}
        />
      ))}

      <div className="relative">
        <div className="flex items-center gap-0.5 border border-dashed border-border rounded-full px-2 py-0.5">
          <Plus size={12} className="text-muted-foreground" />
          <input
            ref={inputRef}
            type="text"
            value={inputValue}
            placeholder="add tag"
            className="text-xs bg-transparent outline-none w-16 placeholder:text-muted-foreground/60"
            onChange={(e) => { setInputValue(e.target.value); setShowDropdown(true); }}
            onFocus={() => setShowDropdown(true)}
            onKeyDown={handleKeyDown}
            disabled={isAdding}
          />
        </div>

        {showDropdown && (inputValue || suggestions.length > 0) && (
          <div className="absolute top-full left-0 mt-1 z-50 bg-popover border border-border rounded-lg shadow-lg py-1 min-w-[140px]">
            {suggestions.map((s) => (
              <button
                key={s}
                type="button"
                className="w-full text-left px-3 py-1.5 text-xs hover:bg-muted transition-colors"
                onMouseDown={(e) => { e.preventDefault(); addTag(s); }}
              >
                {s}
              </button>
            ))}
            {inputValue.trim() && !existingTagNames.includes(inputValue.toLowerCase().trim()) && (
              <button
                type="button"
                className="w-full text-left px-3 py-1.5 text-xs text-muted-foreground hover:bg-muted transition-colors border-t border-border"
                onMouseDown={(e) => { e.preventDefault(); addTag(inputValue); }}
              >
                Add &ldquo;{inputValue.trim()}&rdquo;
              </button>
            )}
          </div>
        )}
      </div>
    </div>
  );
};

export default TagInput;
