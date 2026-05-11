import React from 'react';
import { X } from 'lucide-react';
import TagPill from './TagPill';
import { TagCount } from '../../utils/tags';

interface TagFilterProps {
  availableTags: TagCount[];
  selectedTags: string[];
  onSelectionChange: (tags: string[]) => void;
}

const TagFilter: React.FC<TagFilterProps> = ({ availableTags, selectedTags, onSelectionChange }) => {
  if (availableTags.length === 0) return null;

  const toggle = (tag: string) => {
    if (selectedTags.includes(tag)) {
      onSelectionChange(selectedTags.filter((t) => t !== tag));
    } else {
      onSelectionChange([...selectedTags, tag]);
    }
  };

  const clearAll = () => onSelectionChange([]);

  return (
    <div className="px-3 py-2 border-b border-border">
      <div className="flex flex-wrap gap-1 items-center">
        <span className="text-xs text-muted-foreground mr-1 shrink-0">Filter:</span>
        {availableTags.slice(0, 12).map(({ tag }) => (
          <TagPill
            key={tag}
            tag={tag}
            size="sm"
            active={selectedTags.includes(tag)}
            onClick={() => toggle(tag)}
          />
        ))}
        {selectedTags.length > 0 && (
          <button
            type="button"
            className="inline-flex items-center gap-0.5 text-xs text-muted-foreground hover:text-foreground transition-colors"
            onClick={clearAll}
          >
            <X size={12} />
            clear
          </button>
        )}
      </div>
    </div>
  );
};

export default TagFilter;
