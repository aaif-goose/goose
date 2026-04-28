import { forwardRef, type ComponentType, type MouseEventHandler } from "react";
import { cn } from "@/shared/lib/cn";

interface SidebarNavItemProps {
  icon: ComponentType<{ className?: string }>;
  label: string;
  collapsed: boolean;
  labelTransition: string;
  labelVisible: boolean;
  isActive: boolean;
  onClick: () => void;
  onMouseEnter: MouseEventHandler<HTMLElement>;
  testId?: string;
  itemTransitionDelay?: string;
  labelTransitionDelay?: string;
}

export const SidebarNavItem = forwardRef<
  HTMLButtonElement,
  SidebarNavItemProps
>(function SidebarNavItem(
  {
    icon: Icon,
    label,
    collapsed,
    labelTransition,
    labelVisible,
    isActive,
    onClick,
    onMouseEnter,
    testId,
    itemTransitionDelay,
    labelTransitionDelay,
  },
  ref,
) {
  return (
    <button
      ref={ref}
      type="button"
      data-testid={testId}
      onClick={onClick}
      onMouseEnter={onMouseEnter}
      title={collapsed ? label : undefined}
      aria-label={label}
      aria-current={isActive ? "page" : undefined}
      className={cn(
        "flex items-center w-full text-[13px] transition-colors duration-200 rounded-md",
        "gap-2.5 p-3",
        isActive
          ? "font-medium text-foreground"
          : "text-muted-foreground hover:text-foreground",
      )}
      style={{ transitionDelay: itemTransitionDelay }}
    >
      <Icon className="size-4 flex-shrink-0" />
      <span
        className={cn(
          "whitespace-nowrap",
          labelTransition,
          labelVisible ? "opacity-100 w-auto" : "opacity-0 w-0 overflow-hidden",
        )}
        style={{ transitionDelay: labelTransitionDelay }}
      >
        {label}
      </span>
    </button>
  );
});
