import { cn } from "@/shared/lib/cn";

interface TopBarProps {
  className?: string;
}

export function TopBar({ className }: TopBarProps) {
  return (
    <header
      className={cn(
        "flex h-10 items-center gap-2 bg-background/80 pl-20 pr-3 backdrop-blur-sm",
        className,
      )}
      data-tauri-drag-region
    >
      <div className="min-w-0 flex-1" />
    </header>
  );
}
