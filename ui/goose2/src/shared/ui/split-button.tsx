import type * as React from "react";
import { IconChevronDown } from "@tabler/icons-react";
import { cn } from "@/shared/lib/cn";
import { Button, type ButtonProps } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";

export interface SplitButtonAction<T extends string = string> {
  id: T;
  label: React.ReactNode;
  disabled?: boolean;
}

interface SplitButtonProps<T extends string = string> {
  actions: SplitButtonAction<T>[];
  activeActionId: T;
  onPrimaryClick: (actionId: T) => void;
  onActionSelect: (actionId: T) => void;
  disabled?: boolean;
  className?: string;
  menuTriggerLabel: string;
  variant?: ButtonProps["variant"];
  size?: ButtonProps["size"];
}

export function SplitButton<T extends string = string>({
  actions,
  activeActionId,
  onPrimaryClick,
  onActionSelect,
  disabled = false,
  className,
  menuTriggerLabel,
  variant = "outline-flat",
  size = "xs",
}: SplitButtonProps<T>) {
  const activeAction =
    actions.find((action) => action.id === activeActionId) ?? actions[0];

  if (!activeAction) {
    return null;
  }

  const isPrimaryDisabled = disabled || activeAction.disabled;

  return (
    <div className={cn("inline-flex items-stretch", className)}>
      <Button
        type="button"
        variant={variant}
        size={size}
        disabled={isPrimaryDisabled}
        className="rounded-r-none border-r-0 font-normal"
        onClick={() => onPrimaryClick(activeAction.id)}
      >
        {activeAction.label}
      </Button>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            type="button"
            variant={variant}
            size={size}
            disabled={disabled}
            className="rounded-l-none px-2"
            aria-label={menuTriggerLabel}
            title={menuTriggerLabel}
          >
            <IconChevronDown className="size-3.5" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" sideOffset={4}>
          {actions.map((action) => (
            <DropdownMenuItem
              key={action.id}
              disabled={disabled || action.disabled}
              onSelect={() => {
                onActionSelect(action.id);
                onPrimaryClick(action.id);
              }}
            >
              {action.label}
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
