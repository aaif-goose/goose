import type { ComponentType } from "react";
import { cn } from "@/shared/lib/cn";

interface SidebarNavItemProps {
  icon: ComponentType<{ className?: string }>;
  label: string;
  collapsed: boolean;
  labelTransition: string;
  labelVisible: boolean;
  isActive: boolean;
  onClick: () => void;
  testId?: string;
  itemTransitionDelay?: string;
  labelTransitionDelay?: string;
}

export function SidebarNavItem({
  icon: Icon,
  label,
  collapsed,
  labelTransition,
  labelVisible,
  isActive,
  onClick,
  testId,
  itemTransitionDelay,
  labelTransitionDelay,
}: SidebarNavItemProps) {
  return (
    <button
      type="button"
      data-testid={testId}
      onClick={onClick}
      title={collapsed ? label : undefined}
      aria-label={label}
      aria-current={isActive ? "page" : undefined}
      className={cn(
        "flex items-center w-full text-sm transition-colors duration-200 rounded-md",
        "gap-2.5 px-3 py-1.5",
        isActive
          ? "bg-background-alt font-normal text-foreground"
          : "font-normal text-foreground hover:bg-background-alt hover:text-foreground",
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
}
