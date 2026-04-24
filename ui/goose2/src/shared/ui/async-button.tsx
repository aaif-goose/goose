import * as React from "react";

import { cn } from "@/shared/lib/cn";
import { Button, type ButtonProps } from "@/shared/ui/button";
import { Spinner } from "@/shared/ui/spinner";

export type AsyncButtonState = "idle" | "pending" | "success" | "error";

type PendingVisual = "text" | "spinner" | "spinnerText";

interface AsyncButtonProps extends Omit<ButtonProps, "children" | "asChild"> {
  state: AsyncButtonState;
  idleLabel: React.ReactNode;
  pendingLabel?: React.ReactNode;
  successLabel?: React.ReactNode;
  errorLabel?: React.ReactNode;
  pendingDelayMs?: number;
  pendingVisual?: PendingVisual;
  preserveWidth?: boolean;
}

function getSpinnerClass(size: ButtonProps["size"]) {
  switch (size) {
    case "xs":
    case "sm":
    case "icon-xs":
      return "size-3";
    case "lg":
    case "icon-lg":
      return "size-4";
    default:
      return "size-3.5";
  }
}

function AsyncButton({
  state,
  idleLabel,
  pendingLabel = "Loading...",
  successLabel = "Done",
  errorLabel,
  pendingDelayMs = 250,
  pendingVisual = "text",
  preserveWidth = true,
  disabled,
  size,
  className,
  leftIcon,
  rightIcon,
  ...props
}: AsyncButtonProps) {
  const [displayState, setDisplayState] =
    React.useState<AsyncButtonState>(state);

  React.useEffect(() => {
    if (state !== "pending") {
      setDisplayState(state);
      return;
    }

    if (pendingDelayMs <= 0) {
      setDisplayState("pending");
      return;
    }

    const timer = window.setTimeout(() => {
      setDisplayState("pending");
    }, pendingDelayMs);

    return () => window.clearTimeout(timer);
  }, [pendingDelayMs, state]);

  const spinnerClass = getSpinnerClass(size);
  const labels = {
    idle: idleLabel,
    pending: pendingLabel,
    success: successLabel,
    error: errorLabel ?? idleLabel,
  } satisfies Record<AsyncButtonState, React.ReactNode>;

  function renderStateContent(
    targetState: AsyncButtonState,
    isActive: boolean,
  ) {
    if (targetState === "pending") {
      if (pendingVisual === "spinner") {
        return isActive ? (
          <>
            <Spinner className={spinnerClass} aria-hidden="true" />
            <span className="sr-only">{labels.pending}</span>
          </>
        ) : (
          <span className={cn("inline-block shrink-0", spinnerClass)} />
        );
      }

      if (pendingVisual === "spinnerText") {
        return (
          <>
            {isActive ? (
              <Spinner className={spinnerClass} aria-hidden="true" />
            ) : (
              <span className={cn("inline-block shrink-0", spinnerClass)} />
            )}
            <span>{labels.pending}</span>
          </>
        );
      }
    }

    return <span>{labels[targetState]}</span>;
  }

  const statesToRender = preserveWidth
    ? (["idle", "pending", "success", "error"] as const)
    : ([displayState] as const);

  return (
    <Button
      data-state={displayState}
      aria-busy={state === "pending"}
      disabled={disabled || state === "pending"}
      size={size}
      className={className}
      leftIcon={leftIcon}
      rightIcon={rightIcon}
      {...props}
    >
      <span className="inline-grid items-center justify-items-center">
        {statesToRender.map((targetState) => (
          <span
            key={targetState}
            aria-hidden={targetState !== displayState}
            className={cn(
              "inline-flex items-center justify-center gap-2 whitespace-nowrap [grid-area:1/1] transition-opacity",
              targetState === displayState
                ? "opacity-100"
                : "pointer-events-none opacity-0",
            )}
          >
            {renderStateContent(targetState, targetState === displayState)}
          </span>
        ))}
      </span>
    </Button>
  );
}

export { AsyncButton };
