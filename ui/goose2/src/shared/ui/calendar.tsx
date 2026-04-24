import type * as React from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { DayPicker } from "react-day-picker";

import { cn } from "@/shared/lib/cn";
import { buttonVariants } from "@/shared/ui/button";

function Calendar({
  className,
  classNames,
  showOutsideDays = true,
  ...props
}: React.ComponentProps<typeof DayPicker>) {
  return (
    <DayPicker
      showOutsideDays={showOutsideDays}
      className={cn("p-3", className)}
      classNames={{
        months: "flex flex-col sm:flex-row gap-2",
        month: "flex flex-col gap-4",
        month_caption: "flex justify-center pt-1 relative items-center w-full",
        caption_label: "text-sm font-medium",
        nav: "flex items-center gap-1",
        button_previous: cn(
          buttonVariants({ variant: "outline", size: "icon-xs" }),
          "bg-transparent p-0 absolute left-1",
        ),
        button_next: cn(
          buttonVariants({ variant: "outline", size: "icon-xs" }),
          "bg-transparent p-0 absolute right-1",
        ),
        month_grid: "w-full border-collapse space-x-1",
        weekdays: "flex",
        weekday:
          "text-muted-foreground rounded-md w-8 font-normal text-[0.8rem]",
        week: "flex w-full mt-2",
        day: cn(
          "relative p-0 text-center text-sm focus-within:relative focus-within:z-20",
          props.mode === "range"
            ? "[&:has([aria-selected]):not(:has(.day-range-start)):not(:has(.day-range-end))]:bg-muted first:[&:has([aria-selected])]:rounded-l-full last:[&:has([aria-selected])]:rounded-r-full [&:has(>.day-range-start)]:[background:linear-gradient(to_right,transparent_50%,var(--color-background-muted)_50%)] [&:has(>.day-range-end)]:[background:linear-gradient(to_left,transparent_50%,var(--color-background-muted)_50%)]"
            : "[&:has([aria-selected])]:rounded-full [&:has([aria-selected])]:bg-muted",
        ),
        day_button: cn(
          buttonVariants({ variant: "ghost" }),
          "relative z-10 size-8 p-0 font-normal aria-selected:opacity-100",
        ),
        range_start:
          "day-range-start rounded-full aria-selected:bg-primary aria-selected:text-primary-foreground",
        range_end:
          "day-range-end rounded-full aria-selected:bg-primary aria-selected:text-primary-foreground",
        selected:
          "bg-primary text-primary-foreground hover:bg-primary hover:text-primary-foreground focus:bg-primary focus:text-primary-foreground",
        today: "bg-muted text-muted-foreground",
        outside:
          "day-outside text-muted-foreground aria-selected:text-muted-foreground",
        disabled: "text-muted-foreground opacity-50",
        range_middle:
          "aria-selected:bg-muted aria-selected:text-muted-foreground",
        hidden: "invisible",
        ...classNames,
      }}
      components={{
        Chevron: ({ orientation, ...props }) => {
          if (orientation === "left") {
            return <ChevronLeft className="size-4" {...props} />;
          }
          return <ChevronRight className="size-4" {...props} />;
        },
      }}
      {...props}
    />
  );
}

export { Calendar };
