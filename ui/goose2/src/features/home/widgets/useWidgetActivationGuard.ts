import type { MouseEventHandler, PointerEventHandler } from "react";
import { useCallback, useMemo, useRef } from "react";

const DRAG_ACTIVATION_THRESHOLD = 3;

type Point = {
  x: number;
  y: number;
};

type ActivationMovementEvent = {
  clientX: number;
  clientY: number;
};

type ActivationPointerHandlers = {
  onPointerDown: PointerEventHandler<HTMLButtonElement>;
  onPointerMove: PointerEventHandler<HTMLButtonElement>;
  onPointerUp: PointerEventHandler<HTMLButtonElement>;
  onPointerCancel: PointerEventHandler<HTMLButtonElement>;
  onMouseDown: MouseEventHandler<HTMLButtonElement>;
  onMouseMove: MouseEventHandler<HTMLButtonElement>;
  onMouseUp: MouseEventHandler<HTMLButtonElement>;
};

export function useWidgetActivationGuard(
  shouldIgnoreParentActivation: () => boolean,
) {
  const pointerStartRef = useRef<Point | null>(null);
  const movedRef = useRef(false);

  const markMovedIfNeeded = useCallback((event: ActivationMovementEvent) => {
    const start = pointerStartRef.current;
    if (!start || movedRef.current) {
      return;
    }

    if (
      Math.abs(event.clientX - start.x) > DRAG_ACTIVATION_THRESHOLD ||
      Math.abs(event.clientY - start.y) > DRAG_ACTIVATION_THRESHOLD
    ) {
      movedRef.current = true;
    }
  }, []);

  const pointerHandlers = useMemo<ActivationPointerHandlers>(
    () => ({
      onPointerDown: (event) => {
        movedRef.current = false;
        pointerStartRef.current = {
          x: event.clientX,
          y: event.clientY,
        };
      },
      onPointerMove: markMovedIfNeeded,
      onPointerUp: (event) => {
        markMovedIfNeeded(event);
        pointerStartRef.current = null;
      },
      onPointerCancel: () => {
        movedRef.current = false;
        pointerStartRef.current = null;
      },
      onMouseDown: (event) => {
        movedRef.current = false;
        pointerStartRef.current = {
          x: event.clientX,
          y: event.clientY,
        };
      },
      onMouseMove: markMovedIfNeeded,
      onMouseUp: (event) => {
        markMovedIfNeeded(event);
        pointerStartRef.current = null;
      },
    }),
    [markMovedIfNeeded],
  );

  const shouldIgnoreActivation = useCallback(
    () => shouldIgnoreParentActivation() || movedRef.current,
    [shouldIgnoreParentActivation],
  );

  const clearIgnoredActivation = useCallback(() => {
    window.setTimeout(() => {
      movedRef.current = false;
    }, 0);
  }, []);

  return {
    clearIgnoredActivation,
    pointerHandlers,
    shouldIgnoreActivation,
  };
}
