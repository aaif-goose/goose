import type * as React from "react";
import * as AccordionPrimitive from "@radix-ui/react-accordion";
import { ChevronDownIcon } from "lucide-react";

import { cn } from "@/shared/lib/cn";

function Accordion({
  ...props
}: React.ComponentProps<typeof AccordionPrimitive.Root>) {
  return <AccordionPrimitive.Root data-slot="accordion" {...props} />;
}

function AccordionItem({
  className,
  ...props
}: React.ComponentProps<typeof AccordionPrimitive.Item>) {
  return (
    <AccordionPrimitive.Item
      data-slot="accordion-item"
      className={cn("border-b last:border-b-0", className)}
      {...props}
    />
  );
}

function AccordionTrigger({
  className,
  children,
  indicatorClassName,
  indicatorPosition = "end",
  ...props
}: React.ComponentProps<typeof AccordionPrimitive.Trigger> & {
  indicatorClassName?: string;
  indicatorPosition?: "start" | "end" | "none";
}) {
  const indicator = (
    <ChevronDownIcon
      className={cn(
        "pointer-events-none size-4 shrink-0 text-muted-foreground transition-[opacity,transform] duration-200 group-data-[state=open]/accordion-trigger:rotate-180",
        indicatorClassName,
      )}
    />
  );

  return (
    <AccordionPrimitive.Header className="flex">
      <AccordionPrimitive.Trigger
        data-slot="accordion-trigger"
        className={cn(
          "group/accordion-trigger focus-visible:border-ring focus-visible:ring-ring/50 flex flex-1 items-start justify-between gap-4 rounded-md py-4 text-left text-sm transition-all outline-none hover:underline focus-visible:ring-[1px] disabled:pointer-events-none disabled:opacity-50",
          className,
        )}
        {...props}
      >
        {indicatorPosition === "start" ? indicator : null}
        {children}
        {indicatorPosition === "end" ? indicator : null}
      </AccordionPrimitive.Trigger>
    </AccordionPrimitive.Header>
  );
}

function AccordionContent({
  className,
  children,
  ...props
}: React.ComponentProps<typeof AccordionPrimitive.Content>) {
  return (
    <AccordionPrimitive.Content
      data-slot="accordion-content"
      className="data-[state=closed]:animate-accordion-up data-[state=open]:animate-accordion-down overflow-hidden text-sm"
      {...props}
    >
      <div className={cn("pt-0 pb-4", className)}>{children}</div>
    </AccordionPrimitive.Content>
  );
}

function AccordionSectionTrigger({
  className,
  title,
  meta,
  indicatorClassName,
  ...props
}: Omit<React.ComponentProps<typeof AccordionPrimitive.Trigger>, "children"> & {
  title: React.ReactNode;
  meta?: React.ReactNode;
  indicatorClassName?: string;
}) {
  return (
    <AccordionTrigger
      indicatorPosition="none"
      className={cn("px-5 py-4 text-left hover:no-underline", className)}
      {...props}
    >
      <div className="flex flex-1 items-baseline justify-between gap-3">
        <div className="flex items-center gap-2">
          <p className="text-base font-normal text-foreground">{title}</p>
          <ChevronDownIcon
            className={cn(
              "size-4 shrink-0 text-muted-foreground opacity-0 transition-[opacity,transform] duration-200 group-hover/accordion-trigger:opacity-100 group-focus-visible/accordion-trigger:opacity-100 group-data-[state=open]/accordion-trigger:rotate-180",
              indicatorClassName,
            )}
          />
        </div>
        {meta ? (
          <div className="text-xs font-light text-muted-foreground">{meta}</div>
        ) : null}
      </div>
    </AccordionTrigger>
  );
}

export {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionSectionTrigger,
  AccordionTrigger,
};
