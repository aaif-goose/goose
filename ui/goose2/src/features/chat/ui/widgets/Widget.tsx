import type { ReactNode } from "react";
import { IconChevronDown } from "@tabler/icons-react";
import { cn } from "@/shared/lib/cn";

interface WidgetProps {
  title: ReactNode;
  icon: ReactNode;
  action?: ReactNode;
  flush?: boolean;
  isOpen?: boolean;
  onToggleOpen?: () => void;
  children: ReactNode;
}

const SECTION_HEADER_TEXT_CLASS =
  "min-w-0 overflow-hidden text-[11px] font-medium uppercase tracking-[0.08em] text-foreground-subtle";

export function Widget({
  title,
  icon,
  action,
  flush,
  isOpen = true,
  onToggleOpen,
  children,
}: WidgetProps) {
  const headerTitle = (
    <>
      {onToggleOpen ? (
        <IconChevronDown
          className={cn(
            "size-3 shrink-0 text-foreground-subtle transition-transform duration-150",
            !isOpen && "-rotate-90",
          )}
        />
      ) : null}
      <span className="shrink-0 text-foreground-subtle">{icon}</span>
      <div className={SECTION_HEADER_TEXT_CLASS}>{title}</div>
    </>
  );

  return (
    <section className="pb-3 pt-4 first:pt-3 last:pb-0">
      <div className="px-4">
        <div className="flex min-h-6 items-center justify-between gap-2">
          {onToggleOpen ? (
            <button
              type="button"
              onClick={onToggleOpen}
              aria-expanded={isOpen}
              className="flex min-w-0 flex-1 items-center gap-1.5 rounded-md py-1 text-left transition-colors hover:text-foreground"
            >
              {headerTitle}
            </button>
          ) : (
            <div className="flex min-w-0 flex-1 items-center gap-1.5">
              {headerTitle}
            </div>
          )}
          {action && <div className="shrink-0">{action}</div>}
        </div>
        {isOpen && !flush && (
          <div className="pt-2 text-sm text-foreground">{children}</div>
        )}
      </div>
      {isOpen && flush ? <div className="pt-1.5">{children}</div> : null}
    </section>
  );
}
