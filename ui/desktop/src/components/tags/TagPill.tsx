import React from 'react';
import { X } from 'lucide-react';
import { cn } from '../../utils';

// Map tag names to consistent colors by hashing
const TAG_COLORS = ['blue', 'green', 'amber', 'red', 'purple', 'slate'] as const;
type TagColor = (typeof TAG_COLORS)[number];

function tagToColor(tag: string): TagColor {
  let hash = 0;
  for (let i = 0; i < tag.length; i++) {
    hash = (hash * 31 + tag.charCodeAt(i)) & 0xffffff;
  }
  return TAG_COLORS[Math.abs(hash) % TAG_COLORS.length];
}

const COLOR_CLASSES: Record<TagColor, string> = {
  blue:   'bg-blue-100 text-blue-700 dark:bg-blue-900/40 dark:text-blue-300 border-blue-200 dark:border-blue-800',
  green:  'bg-green-100 text-green-700 dark:bg-green-900/40 dark:text-green-300 border-green-200 dark:border-green-800',
  amber:  'bg-amber-100 text-amber-700 dark:bg-amber-900/40 dark:text-amber-300 border-amber-200 dark:border-amber-800',
  red:    'bg-red-100 text-red-700 dark:bg-red-900/40 dark:text-red-300 border-red-200 dark:border-red-800',
  purple: 'bg-purple-100 text-purple-700 dark:bg-purple-900/40 dark:text-purple-300 border-purple-200 dark:border-purple-800',
  slate:  'bg-slate-100 text-slate-700 dark:bg-slate-800/60 dark:text-slate-300 border-slate-200 dark:border-slate-700',
};

interface TagPillProps {
  tag: string;
  onRemove?: () => void;
  onClick?: () => void;
  size?: 'sm' | 'md';
  active?: boolean;
}

const TagPill: React.FC<TagPillProps> = ({ tag, onRemove, onClick, size = 'md', active = false }) => {
  const color  = tagToColor(tag);
  const colors = COLOR_CLASSES[color];

  const sizeClasses = size === 'sm'
    ? 'px-1.5 py-0.5 text-xs gap-0.5'
    : 'px-2 py-1 text-xs gap-1';

  return (
    <span
      className={cn(
        'inline-flex items-center rounded-full border font-medium transition-colors',
        sizeClasses,
        colors,
        onClick ? 'cursor-pointer hover:opacity-80' : '',
        active ? 'ring-2 ring-offset-1 ring-current' : '',
      )}
      onClick={onClick}
    >
      {tag}
      {onRemove && (
        <button
          type="button"
          className="ml-0.5 rounded-full hover:bg-black/10 dark:hover:bg-white/10 p-0.5"
          onClick={(e) => { e.stopPropagation(); onRemove(); }}
          aria-label={`Remove tag ${tag}`}
        >
          <X size={10} />
        </button>
      )}
    </span>
  );
};

export default TagPill;
