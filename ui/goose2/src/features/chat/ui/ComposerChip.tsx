import { X } from "lucide-react";
import type { ReactNode } from "react";
import { cn } from "@/shared/lib/cn";

type ComposerChipTone = "file" | "agent" | "skill" | "automation";

const toneClasses: Record<ComposerChipTone, string> = {
  file: "bg-gray-100/70 text-gray-600 hover:bg-gray-100 dark:bg-gray-800 dark:text-gray-400 dark:hover:bg-gray-700",
  agent:
    "bg-blue-100/20 text-blue-700 hover:bg-blue-100/30 dark:bg-blue-100/10 dark:text-blue-100 dark:hover:bg-blue-100/15",
  skill:
    "bg-yellow-100/25 text-yellow-700 hover:bg-yellow-100/35 dark:bg-yellow-100/10 dark:text-yellow-100 dark:hover:bg-yellow-100/15",
  automation:
    "bg-green-100/20 text-green-700 hover:bg-green-100/30 dark:bg-green-100/10 dark:text-green-100 dark:hover:bg-green-100/15",
};

interface ComposerChipProps {
  tone: ComposerChipTone;
  label: string;
  removeLabel: string;
  onRemove: () => void;
  leading?: ReactNode;
  title?: string;
  className?: string;
}

export function ComposerChip({
  tone,
  label,
  removeLabel,
  onRemove,
  leading,
  title,
  className,
}: ComposerChipProps) {
  return (
    <span
      className={cn(
        "group inline-flex h-6 max-w-64 items-center rounded-full pl-[9px] pr-2 text-xs font-normal transition-[background-color,color,padding-right] hover:pr-1 focus-within:pr-1",
        toneClasses[tone],
        className,
      )}
      title={title ?? label}
    >
      <span className="flex min-w-0 items-center gap-1.5">
        {leading ? (
          <span className="flex shrink-0 items-center justify-center">
            {leading}
          </span>
        ) : null}
        <span className="min-w-0 truncate">{label}</span>
      </span>
      <button
        type="button"
        onClick={onRemove}
        className="-mr-px flex h-5 w-0 shrink-0 items-center justify-center overflow-hidden rounded-full text-current opacity-0 transition-[width,opacity] hover:bg-transparent hover:opacity-100 focus-visible:w-5 focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring group-hover:w-5 group-hover:opacity-45 group-focus-within:w-5 group-focus-within:opacity-45"
        aria-label={removeLabel}
      >
        <X className="size-3.5" />
      </button>
    </span>
  );
}
