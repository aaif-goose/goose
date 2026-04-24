import type * as React from "react";
import { Search } from "lucide-react";
import { cn } from "@/shared/lib/cn";
import { Input } from "@/shared/ui/input";

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
}

export function SearchBar({
  value,
  onChange,
  placeholder,
  onKeyDown,
  className,
}: SearchBarProps) {
  return (
    <div className={cn("relative", className)}>
      <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
      <Input
        type="search"
        autoComplete="off"
        spellCheck={false}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={onKeyDown}
        placeholder={placeholder}
        className="rounded-lg bg-background pr-3 pl-9 text-sm"
      />
    </div>
  );
}
