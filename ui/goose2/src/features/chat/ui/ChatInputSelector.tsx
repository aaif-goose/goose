import type { ReactNode } from "react";
import { Check, ChevronDown } from "lucide-react";
import { Button } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { cn } from "@/shared/lib/cn";

export interface ChatInputSelectorItem {
  value: string;
  label: string;
  description?: string;
  icon?: ReactNode;
  disabled?: boolean;
}

interface ChatInputSelectorSection {
  label?: string;
  items: ChatInputSelectorItem[];
}

interface ChatInputSelectorProps {
  ariaLabel: string;
  value: string;
  triggerLabel: string;
  triggerTitle?: string;
  icon?: ReactNode;
  triggerVariant?: "default" | "toolbar";
  triggerSize?: "default" | "sm";
  menuLabel?: string;
  sections: ChatInputSelectorSection[];
  onValueChange: (value: string) => void;
  contentWidth?: "trigger" | "wide";
  disabled?: boolean;
}

export function ChatInputSelector({
  ariaLabel,
  value,
  triggerLabel,
  triggerTitle,
  icon,
  triggerVariant = "default",
  triggerSize = "default",
  menuLabel,
  sections,
  onValueChange,
  contentWidth = "trigger",
  disabled,
}: ChatInputSelectorProps) {
  const buttonSize = triggerSize === "sm" ? "xs" : "sm";

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          type="button"
          variant={triggerVariant === "toolbar" ? "toolbar" : "outline"}
          size={buttonSize}
          aria-label={ariaLabel}
          title={triggerTitle}
          disabled={disabled}
          leftIcon={icon}
          rightIcon={<ChevronDown />}
          className={cn(
            "min-w-0",
            triggerVariant === "default" && "justify-between",
          )}
        >
          <span className="truncate">{triggerLabel}</span>
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        align="start"
        className={cn(contentWidth === "wide" ? "w-72" : "w-56")}
      >
        {menuLabel ? (
          <>
            <DropdownMenuGroup>
              <DropdownMenuLabel>{menuLabel}</DropdownMenuLabel>
            </DropdownMenuGroup>
            <DropdownMenuSeparator />
          </>
        ) : null}
        {sections.map((section, sectionIndex) => (
          <DropdownMenuGroup
            key={
              section.label ??
              `${ariaLabel}-${section.items.map((item) => item.value).join("|")}`
            }
          >
            {section.label ? (
              <DropdownMenuLabel>{section.label}</DropdownMenuLabel>
            ) : null}
            {section.items.map((item) => (
              <DropdownMenuItem
                key={item.value}
                disabled={item.disabled}
                onSelect={() => onValueChange(item.value)}
                className="justify-between gap-2"
              >
                <div
                  className={cn(
                    "min-w-0 flex flex-1 gap-2",
                    item.description ? "items-start" : "items-center",
                  )}
                >
                  {item.icon ? (
                    <span className="shrink-0">{item.icon}</span>
                  ) : null}
                  <div className="min-w-0 flex-1">
                    <span className="block truncate font-normal">
                      {item.label}
                    </span>
                    {item.description ? (
                      <span className="block truncate text-xs text-muted-foreground">
                        {item.description}
                      </span>
                    ) : null}
                  </div>
                </div>
                {item.value === value ? (
                  <Check className="size-4 shrink-0 self-center text-muted-foreground" />
                ) : null}
              </DropdownMenuItem>
            ))}
            {sectionIndex < sections.length - 1 ? (
              <DropdownMenuSeparator />
            ) : null}
          </DropdownMenuGroup>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
