import type * as React from "react";
import { Search } from "lucide-react";
import { cn } from "@/shared/lib/cn";
import { Input } from "@/shared/ui/input";

const searchBarSizes = {
  compact: {
    wrapper:
      "rounded-md border border-border-soft px-2 py-1 text-xs text-muted-foreground hover:bg-transparent hover:text-foreground",
    icon: "left-2.5 size-3",
    input:
      "h-auto border-none bg-transparent px-0 pl-5 pr-0 text-[11px] font-normal shadow-none focus-visible:border-transparent focus-visible:ring-0 focus-visible:ring-offset-0",
    inputVariant: "ghost" as const,
  },
  small: {
    wrapper:
      "rounded-md border border-border-soft px-2.5 py-1.5 text-xs text-muted-foreground hover:bg-transparent hover:text-foreground",
    icon: "left-3 size-3.5",
    input:
      "h-auto border-none bg-transparent px-0 pl-6 pr-0 text-xs font-normal shadow-none focus-visible:border-transparent focus-visible:ring-0 focus-visible:ring-offset-0",
    inputVariant: "ghost" as const,
  },
  default: {
    wrapper: "",
    icon: "left-3 size-4",
    input:
      "rounded-lg border-border-soft bg-background pr-3 pl-9 text-sm font-normal hover:border-border-soft focus-visible:border-ring",
    inputVariant: "default" as const,
  },
} as const;

interface SearchBarProps {
  /** Current search value (controlled) */
  value: string;
  /** Called when the search term changes */
  onChange: (term: string) => void;
  /** Placeholder text */
  placeholder?: string;
  /** Optional keydown handler for the input */
  onKeyDown?: React.KeyboardEventHandler<HTMLInputElement>;
  /** Optional className for the wrapper */
  className?: string;
  /** Size variant for the search field */
  size?: keyof typeof searchBarSizes;
  /** Optional ref for the underlying input */
  inputRef?: React.Ref<HTMLInputElement>;
  /** Accessible label for the search input */
  "aria-label"?: string;
}

export function SearchBar({
  value,
  onChange,
  placeholder,
  onKeyDown,
  className,
  size = "default",
  inputRef,
  "aria-label": ariaLabel,
}: SearchBarProps) {
  const styles = searchBarSizes[size];

  return (
    <div className={cn("relative w-full", styles.wrapper, className)}>
      <Search
        className={cn(
          "pointer-events-none absolute top-1/2 -translate-y-1/2 text-placeholder",
          styles.icon,
        )}
      />
      <Input
        inputRef={inputRef}
        variant={styles.inputVariant}
        type="search"
        autoComplete="off"
        autoCorrect="off"
        autoCapitalize="none"
        spellCheck={false}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={onKeyDown}
        placeholder={placeholder}
        aria-label={ariaLabel}
        className={cn("w-full placeholder:text-placeholder", styles.input)}
      />
    </div>
  );
}
