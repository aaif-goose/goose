import { useControllableState } from "@radix-ui/react-use-controllable-state";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/shared/ui/collapsible";
import { cn } from "@/shared/lib/cn";
import type { DynamicToolUIPart, ToolUIPart } from "ai";
import {
  CheckCircleIcon,
  ChevronDownIcon,
  CircleIcon,
  ClockIcon,
  WrenchIcon,
  XCircleIcon,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import type { ComponentProps, ReactNode } from "react";
import {
  createContext,
  isValidElement,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import { CodeBlock } from "./code-block";

export type ToolProps = ComponentProps<typeof Collapsible>;

interface ToolContextValue {
  isOpen: boolean;
  setIsOpen: (open: boolean) => void;
}

const ToolContext = createContext<ToolContextValue | null>(null);

export const Tool = ({
  className,
  open,
  defaultOpen = false,
  onOpenChange,
  ...props
}: ToolProps) => {
  const [isOpen, setIsOpen] = useControllableState({
    defaultProp: defaultOpen,
    onChange: onOpenChange,
    prop: open,
  });
  const value = useMemo(() => ({ isOpen, setIsOpen }), [isOpen, setIsOpen]);

  return (
    <ToolContext.Provider value={value}>
      <Collapsible
        className={cn("group not-prose w-full min-w-0 max-w-full", className)}
        open={isOpen}
        onOpenChange={setIsOpen}
        {...props}
      />
    </ToolContext.Provider>
  );
};

export type ToolPart = ToolUIPart | DynamicToolUIPart;

export type ToolHeaderProps = {
  title?: ReactNode;
  className?: string;
  showIcon?: boolean;
  showStatusBadge?: boolean;
  /** When false, hides the trailing disclosure chevron in the header. */
  showChevron?: boolean;
  splitTrigger?: boolean;
  layout?: "fill" | "fit";
  elapsedSeconds?: number;
} & (
  | { type: ToolUIPart["type"]; state: ToolUIPart["state"]; toolName?: never }
  | {
      type: DynamicToolUIPart["type"];
      state: DynamicToolUIPart["state"];
      toolName: string;
    }
);

const statusLabels: Record<ToolPart["state"], string> = {
  "approval-requested": "Awaiting Approval",
  "approval-responded": "Responded",
  "input-available": "Running",
  "input-streaming": "Pending",
  "output-available": "Completed",
  "output-denied": "Denied",
  "output-error": "Error",
};

const statusIconComponents: Record<ToolPart["state"], LucideIcon> = {
  "approval-requested": ClockIcon,
  "approval-responded": CheckCircleIcon,
  "input-available": ClockIcon,
  "input-streaming": CircleIcon,
  "output-available": CheckCircleIcon,
  "output-denied": XCircleIcon,
  "output-error": XCircleIcon,
};

const statusIconClasses: Record<ToolPart["state"], string> = {
  "approval-requested": "text-yellow-600",
  "approval-responded": "text-blue-600",
  "input-available": "animate-pulse",
  "input-streaming": "",
  "output-available": "text-green-600",
  "output-denied": "text-orange-600",
  "output-error": "text-red-600",
};

export const ToolStatusIcon = ({
  status,
  className,
}: {
  status: ToolPart["state"];
  className?: string;
}) => {
  const Icon = statusIconComponents[status];
  return (
    <Icon
      aria-hidden="true"
      className={cn("size-4 shrink-0", statusIconClasses[status], className)}
    />
  );
};

export const getStatusBadge = (
  status: ToolPart["state"],
  className?: string,
) => {
  if (status === "output-available") return null;
  return (
    <span
      className={cn(
        "shrink-0 flex items-center gap-1 text-xs text-muted-foreground",
        className,
      )}
    >
      <ToolStatusIcon status={status} />
      {statusLabels[status]}
    </span>
  );
};

export const ToolHeader = ({
  className,
  title,
  type,
  state,
  toolName,
  showIcon = true,
  showStatusBadge = true,
  showChevron = true,
  splitTrigger = false,
  layout = "fill",
  elapsedSeconds,
  ...props
}: ToolHeaderProps) => {
  const derivedName =
    type === "dynamic-tool" ? toolName : type.split("-").slice(1).join("-");
  const isFitLayout = layout === "fit";
  const toolContext = useContext(ToolContext);
  const isOpen = toolContext?.isOpen ?? false;

  const containerClasses = cn(
    "items-center gap-1.5 py-px",
    isFitLayout ? "inline-flex w-fit max-w-full self-start" : "flex w-full",
    className,
  );

  const titleClasses = cn(
    "min-w-0 truncate text-left text-sm font-medium",
    isFitLayout ? "flex-none max-w-full" : "flex-1",
  );

  const trailing = (
    <>
      {showStatusBadge && getStatusBadge(state)}
      {elapsedSeconds != null && (
        <span className="shrink-0 tabular-nums text-xs text-muted-foreground">
          {elapsedSeconds}s
        </span>
      )}
      {showChevron && (
        <ChevronDownIcon className="size-3.5 shrink-0 text-muted-foreground transition-transform group-data-[state=closed]:-rotate-90" />
      )}
    </>
  );

  if (splitTrigger) {
    return (
      <div className={containerClasses}>
        <div
          className={cn("min-w-0 cursor-pointer items-center gap-1.5", {
            "flex flex-1": !isFitLayout,
            "inline-flex max-w-full": isFitLayout,
          })}
          role="button"
          tabIndex={0}
          onClick={() => toolContext?.setIsOpen(!isOpen)}
          onKeyDown={(event) => {
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault();
              toolContext?.setIsOpen(!isOpen);
            }
          }}
        >
          {showIcon && (
            <WrenchIcon className="size-4 shrink-0 text-muted-foreground" />
          )}
          <span className={titleClasses}>{title ?? derivedName}</span>
        </div>
        <CollapsibleTrigger className="shrink-0 flex items-center gap-1.5">
          {trailing}
        </CollapsibleTrigger>
      </div>
    );
  }

  return (
    <CollapsibleTrigger className={containerClasses} {...props}>
      {showIcon && (
        <WrenchIcon className="size-4 shrink-0 text-muted-foreground" />
      )}
      <span className={titleClasses}>{title ?? derivedName}</span>
      <span className="shrink-0 flex items-center gap-1.5">{trailing}</span>
    </CollapsibleTrigger>
  );
};

export type ToolContentProps = ComponentProps<typeof CollapsibleContent>;

export const ToolContent = ({ className, ...props }: ToolContentProps) => (
  <CollapsibleContent
    className={cn(
      "data-[state=closed]:fade-out-0 data-[state=closed]:slide-out-to-top-2 data-[state=open]:slide-in-from-top-2 min-w-0 max-w-full space-y-2 py-2 text-popover-foreground outline-none data-[state=closed]:animate-out data-[state=open]:animate-in",
      className,
    )}
    {...props}
  />
);

export type ToolSectionProps = ComponentProps<"div"> & {
  label: string;
};

export const ToolSection = ({
  className,
  label,
  children,
  ...props
}: ToolSectionProps) => (
  <div className={cn("space-y-2", className)} {...props}>
    <h4 className="font-medium text-muted-foreground text-xs uppercase tracking-wide">
      {label}
    </h4>
    {children}
  </div>
);

export type ToolSurfaceProps = ComponentProps<"div"> & {
  destructive?: boolean;
  tone?: "muted" | "outline";
};

export const ToolSurface = ({
  className,
  destructive = false,
  tone = "muted",
  ...props
}: ToolSurfaceProps) => (
  <div
    className={cn(
      "overflow-hidden rounded-md text-xs [&_pre]:whitespace-pre-wrap [&_pre]:break-words",
      destructive
        ? "bg-destructive/10 text-destructive"
        : tone === "outline"
          ? "border border-border bg-background text-foreground"
          : "bg-muted/50 text-foreground",
      className,
    )}
    {...props}
  />
);

function EmbeddedOverflowViewport({
  className,
  children,
}: {
  className?: string;
  children: ReactNode;
}) {
  const viewportRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const [showTopFade, setShowTopFade] = useState(false);
  const [showBottomFade, setShowBottomFade] = useState(false);

  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;

    const updateFadeState = () => {
      const { scrollTop, scrollHeight, clientHeight } = viewport;
      const hasOverflow = scrollHeight - clientHeight > 1;
      setShowTopFade(hasOverflow && scrollTop > 1);
      setShowBottomFade(
        hasOverflow && scrollTop + clientHeight < scrollHeight - 1,
      );
    };

    updateFadeState();
    viewport.addEventListener("scroll", updateFadeState, { passive: true });
    window.addEventListener("resize", updateFadeState);

    let resizeObserver: ResizeObserver | undefined;
    if (typeof ResizeObserver !== "undefined") {
      resizeObserver = new ResizeObserver(() => updateFadeState());
      resizeObserver.observe(viewport);
      if (contentRef.current) {
        resizeObserver.observe(contentRef.current);
      }
    }

    return () => {
      viewport.removeEventListener("scroll", updateFadeState);
      window.removeEventListener("resize", updateFadeState);
      resizeObserver?.disconnect();
    };
  }, []);

  return (
    <div className="relative">
      <div ref={viewportRef} className={className}>
        <div ref={contentRef}>{children}</div>
      </div>
      {showTopFade && (
        <div className="pointer-events-none absolute inset-x-0 top-0 z-10 h-6 bg-gradient-to-b from-muted to-transparent" />
      )}
      {showBottomFade && (
        <div className="pointer-events-none absolute inset-x-0 bottom-0 z-10 h-6 bg-gradient-to-t from-muted to-transparent" />
      )}
    </div>
  );
}

export type ToolInputProps = ComponentProps<"div"> & {
  input: ToolPart["input"];
  label?: string;
  showLabel?: boolean;
  summary?: ReactNode | ((options: { isOpen: boolean }) => ReactNode);
  embedded?: boolean;
};

export const ToolInput = ({
  className,
  input,
  label = "Parameters",
  showLabel = true,
  summary,
  embedded = false,
  ...props
}: ToolInputProps) => {
  const [isJsonOpen, setIsJsonOpen] = useState(false);
  const hasStructuredInput =
    input !== undefined &&
    input !== null &&
    (typeof input !== "object" ||
      Array.isArray(input) ||
      Object.keys(input as Record<string, unknown>).length > 0);

  if (!summary && !hasStructuredInput) {
    return null;
  }

  const summaryContent =
    typeof summary === "function"
      ? summary({ isOpen: isJsonOpen })
      : (summary ?? (
          <span className="text-xs text-muted-foreground">{label}</span>
        ));

  const inputBody = hasStructuredInput ? (
    <Collapsible open={isJsonOpen} onOpenChange={setIsJsonOpen}>
      <CollapsibleTrigger asChild>
        <button
          type="button"
          className="flex w-full items-start gap-3 text-left"
        >
          <div className="min-w-0 flex-1">{summaryContent}</div>
          <ChevronDownIcon className="mt-0.5 size-3.5 shrink-0 text-muted-foreground transition-transform [button[data-state=closed]_&]:-rotate-90" />
        </button>
      </CollapsibleTrigger>
      <CollapsibleContent className="data-[state=closed]:fade-out-0 data-[state=closed]:slide-out-to-top-1 data-[state=open]:slide-in-from-top-1 mt-2 overflow-hidden outline-none data-[state=closed]:animate-out data-[state=open]:animate-in">
        <pre className="overflow-auto whitespace-pre-wrap break-words font-mono text-[12px] leading-5 text-foreground/90">
          {JSON.stringify(input, null, 2)}
        </pre>
      </CollapsibleContent>
    </Collapsible>
  ) : (
    summaryContent
  );

  if (embedded) {
    return (
      <div className={cn("overflow-hidden px-3 py-2", className)} {...props}>
        {inputBody}
      </div>
    );
  }

  if (!showLabel) {
    return (
      <div className={cn("space-y-2 overflow-hidden", className)} {...props}>
        <ToolSurface tone="muted" className="px-3 py-2">
          {inputBody}
        </ToolSurface>
      </div>
    );
  }

  return (
    <ToolSection
      label={label}
      className={cn("overflow-hidden", className)}
      {...props}
    >
      <ToolSurface tone="muted" className="px-3 py-2">
        {inputBody}
      </ToolSurface>
    </ToolSection>
  );
};

export type ToolOutputProps = ComponentProps<"div"> & {
  output: ToolPart["output"];
  errorText: ToolPart["errorText"];
  label?: string;
  contentClassName?: string;
  showLabel?: boolean;
  embedded?: boolean;
  /** Max height (Tailwind class, e.g. "max-h-32") for the embedded scroll viewport. */
  embeddedMaxHeightClass?: string;
};

export const ToolOutput = ({
  className,
  output,
  errorText,
  label,
  contentClassName,
  showLabel = true,
  embedded = false,
  embeddedMaxHeightClass = "max-h-32",
  ...props
}: ToolOutputProps) => {
  if (output === undefined && errorText === undefined) {
    return null;
  }

  const renderedOutput = (() => {
    if (typeof output === "object" && !isValidElement(output)) {
      return (
        <CodeBlock
          code={JSON.stringify(output, null, 2)}
          language="json"
          className={
            embedded
              ? "rounded-none border-0 bg-transparent shadow-none [&_pre]:m-0 [&_pre]:bg-transparent [&_pre]:p-0"
              : undefined
          }
        />
      );
    }
    if (typeof output === "string") {
      return (
        <CodeBlock
          code={output}
          language="json"
          className={
            embedded
              ? "rounded-none border-0 bg-transparent shadow-none [&_pre]:m-0 [&_pre]:bg-transparent [&_pre]:p-0"
              : undefined
          }
        />
      );
    }
    return <div>{output as ReactNode}</div>;
  })();

  if (embedded) {
    const plainTextClasses =
      "m-0 whitespace-pre-wrap break-words font-mono text-[12px] leading-5";
    const plainOutput = errorText
      ? errorText
      : typeof output === "string"
        ? output
        : typeof output === "object" && !isValidElement(output)
          ? JSON.stringify(output, null, 2)
          : null;

    return (
      <div
        data-role="tool-output-embedded"
        className={cn("overflow-hidden px-3 pb-2", className)}
        {...props}
      >
        {plainOutput != null ? (
          <EmbeddedOverflowViewport
            className={cn("overflow-auto", embeddedMaxHeightClass)}
          >
            <pre
              className={cn(
                plainTextClasses,
                errorText ? "text-destructive" : "text-muted-foreground",
              )}
            >
              {plainOutput}
            </pre>
          </EmbeddedOverflowViewport>
        ) : (
          <EmbeddedOverflowViewport
            className={cn(
              "overflow-auto text-muted-foreground",
              embeddedMaxHeightClass,
            )}
          >
            {renderedOutput}
          </EmbeddedOverflowViewport>
        )}
      </div>
    );
  }

  if (!showLabel) {
    return (
      <div className={cn("space-y-2", className)} {...props}>
        {errorText ? (
          <ToolSurface destructive className="px-3 py-2">
            <div>{errorText}</div>
          </ToolSurface>
        ) : (
          <ToolSurface tone="muted" className="px-3 py-2">
            {renderedOutput}
          </ToolSurface>
        )}
      </div>
    );
  }

  return (
    <ToolSection
      label={label ?? (errorText ? "Error" : "Result")}
      className={className}
      {...props}
    >
      <ToolSurface
        destructive={Boolean(errorText)}
        tone="muted"
        className={cn("overflow-x-auto [&_table]:w-full", contentClassName)}
      >
        {errorText && <div>{errorText}</div>}
        {renderedOutput}
      </ToolSurface>
    </ToolSection>
  );
};
