import type { RefObject } from "react";
import { useCallback, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { motion } from "motion/react";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@/shared/ui/context-menu";
import { HOME_WIDGET_CATALOG_BY_ID } from "../widgets/catalog";
import type {
  CanvasBounds,
  WidgetInstance,
  WidgetNavigationHandlers,
} from "../widgets/types";
import { useHomeWidgetStore } from "../stores/homeWidgetStore";

interface WidgetFrameProps extends WidgetNavigationHandlers {
  instance: WidgetInstance;
  canvasRef: RefObject<HTMLDivElement | null>;
  currentMaxZ: number;
  getCanvasBounds: () => CanvasBounds | undefined;
}

export function WidgetFrame({
  instance,
  canvasRef,
  currentMaxZ,
  getCanvasBounds,
  onOpenAgent,
  onStartChatWithPersona,
  onSelectSession,
}: WidgetFrameProps) {
  const { t } = useTranslation("home");
  const moveWidget = useHomeWidgetStore((state) => state.moveWidget);
  const bumpZ = useHomeWidgetStore((state) => state.bumpZ);
  const removeWidget = useHomeWidgetStore((state) => state.removeWidget);
  const updateWidgetState = useHomeWidgetStore(
    (state) => state.updateWidgetState,
  );
  const suppressClickRef = useRef(false);
  const clickSuppressionTimerRef = useRef<number | null>(null);
  const removeClickBlockerRef = useRef<(() => void) | null>(null);
  const pointerStartRef = useRef<{ x: number; y: number } | null>(null);
  const didDragRef = useRef(false);
  const catalogEntry = HOME_WIDGET_CATALOG_BY_ID[instance.type];

  const handleUpdateState = useCallback(
    (next: Record<string, unknown>) => updateWidgetState(instance.id, next),
    [instance.id, updateWidgetState],
  );

  const blockNextClick = useCallback(() => {
    removeClickBlockerRef.current?.();

    const preventNextClick = (event: MouseEvent) => {
      if (!suppressClickRef.current) {
        return;
      }

      event.preventDefault();
      event.stopPropagation();
      event.stopImmediatePropagation();
      removeClickBlockerRef.current = null;
    };

    window.addEventListener("click", preventNextClick, {
      capture: true,
      once: true,
    });
    removeClickBlockerRef.current = () => {
      window.removeEventListener("click", preventNextClick, {
        capture: true,
      });
      removeClickBlockerRef.current = null;
    };
  }, []);

  const suppressClickBriefly = useCallback(() => {
    suppressClickRef.current = true;
    blockNextClick();
    if (clickSuppressionTimerRef.current) {
      window.clearTimeout(clickSuppressionTimerRef.current);
    }
    clickSuppressionTimerRef.current = window.setTimeout(() => {
      suppressClickRef.current = false;
      didDragRef.current = false;
      clickSuppressionTimerRef.current = null;
      removeClickBlockerRef.current?.();
    }, 600);
  }, [blockNextClick]);

  const shouldIgnoreActivation = useCallback(
    () => suppressClickRef.current || didDragRef.current,
    [],
  );

  useEffect(
    () => () => {
      if (clickSuppressionTimerRef.current) {
        window.clearTimeout(clickSuppressionTimerRef.current);
      }
      removeClickBlockerRef.current?.();
    },
    [],
  );

  if (!catalogEntry) {
    return null;
  }

  const Component = catalogEntry.Component;

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <motion.div
          drag
          dragConstraints={canvasRef}
          dragElastic={0}
          dragMomentum={false}
          initial={false}
          exit={{ opacity: 0 }}
          transition={{ type: "spring", stiffness: 430, damping: 32 }}
          onDragStart={suppressClickBriefly}
          onPointerDown={() => {
            if (instance.z < currentMaxZ) {
              bumpZ(instance.id);
            }
          }}
          onPointerDownCapture={(event) => {
            didDragRef.current = false;
            pointerStartRef.current = {
              x: event.clientX,
              y: event.clientY,
            };
          }}
          onPointerMoveCapture={(event) => {
            const start = pointerStartRef.current;
            if (!start || didDragRef.current) {
              return;
            }

            if (
              Math.abs(event.clientX - start.x) > 3 ||
              Math.abs(event.clientY - start.y) > 3
            ) {
              didDragRef.current = true;
              suppressClickBriefly();
            }
          }}
          onPointerUpCapture={(event) => {
            const start = pointerStartRef.current;
            pointerStartRef.current = null;
            if (!start) {
              return;
            }

            if (
              didDragRef.current ||
              Math.abs(event.clientX - start.x) > 3 ||
              Math.abs(event.clientY - start.y) > 3
            ) {
              didDragRef.current = true;
              suppressClickBriefly();
            }
          }}
          onDragEnd={(_, info) => {
            if (Math.abs(info.offset.x) > 3 || Math.abs(info.offset.y) > 3) {
              didDragRef.current = true;
              suppressClickBriefly();
            }
            moveWidget(
              instance.id,
              instance.x + info.offset.x,
              instance.y + info.offset.y,
              getCanvasBounds(),
            );
          }}
          onClickCapture={(event) => {
            if (!suppressClickRef.current) {
              return;
            }
            event.preventDefault();
            event.stopPropagation();
          }}
          style={{
            x: instance.x,
            y: instance.y,
            zIndex: instance.z,
            width: catalogEntry.defaultSize.width,
            height: catalogEntry.defaultSize.height,
          }}
          className="absolute left-0 top-0 cursor-grab select-none touch-none active:cursor-grabbing"
        >
          <Component
            instance={instance}
            onUpdateState={handleUpdateState}
            shouldIgnoreActivation={shouldIgnoreActivation}
            onOpenAgent={onOpenAgent}
            onStartChatWithPersona={onStartChatWithPersona}
            onSelectSession={onSelectSession}
          />
        </motion.div>
      </ContextMenuTrigger>
      <ContextMenuContent>
        <ContextMenuItem
          variant="destructive"
          onSelect={() => removeWidget(instance.id)}
        >
          {t("widgets.actions.remove")}
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}
