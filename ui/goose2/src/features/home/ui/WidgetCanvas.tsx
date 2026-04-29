import { useCallback, useMemo, useRef, useState } from "react";
import { AnimatePresence } from "motion/react";
import { HOME_WIDGET_CATALOG_BY_ID } from "../widgets/catalog";
import type {
  CanvasBounds,
  WidgetInstance,
  WidgetNavigationHandlers,
} from "../widgets/types";
import { useHomeWidgetStore } from "../stores/homeWidgetStore";
import { WidgetFrame } from "./WidgetFrame";
import { WidgetPicker } from "./WidgetPicker";

interface WidgetCanvasProps extends WidgetNavigationHandlers {
  instances: WidgetInstance[];
}

interface PickerState {
  open: boolean;
  x: number;
  y: number;
}

export function WidgetCanvas({
  instances,
  onOpenAgent,
  onStartChatWithPersona,
  onSelectSession,
}: WidgetCanvasProps) {
  const canvasRef = useRef<HTMLDivElement | null>(null);
  const addWidget = useHomeWidgetStore((state) => state.addWidget);
  const [picker, setPicker] = useState<PickerState>({
    open: false,
    x: 0,
    y: 0,
  });

  const getCanvasBounds = useCallback((): CanvasBounds | undefined => {
    const rect = canvasRef.current?.getBoundingClientRect();
    return rect ? { width: rect.width, height: rect.height } : undefined;
  }, []);

  const currentMaxZ = useMemo(
    () => instances.reduce((max, instance) => Math.max(max, instance.z), 0),
    [instances],
  );

  const handleDoubleClick = (event: React.MouseEvent<HTMLDivElement>) => {
    if (event.target !== event.currentTarget) {
      return;
    }

    const rect = event.currentTarget.getBoundingClientRect();
    setPicker({
      open: true,
      x: event.clientX - rect.left,
      y: event.clientY - rect.top,
    });
  };

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: freeform canvas opens the widget picker on empty-space double-click
    <div
      ref={canvasRef}
      onDoubleClick={handleDoubleClick}
      className="relative h-full w-full overflow-hidden bg-dot-grid"
    >
      <AnimatePresence initial={false}>
        {instances
          .filter((instance) => HOME_WIDGET_CATALOG_BY_ID[instance.type])
          .map((instance) => (
            <WidgetFrame
              key={instance.id}
              instance={instance}
              canvasRef={canvasRef}
              currentMaxZ={currentMaxZ}
              getCanvasBounds={getCanvasBounds}
              onOpenAgent={onOpenAgent}
              onStartChatWithPersona={onStartChatWithPersona}
              onSelectSession={onSelectSession}
            />
          ))}
      </AnimatePresence>

      <WidgetPicker
        open={picker.open}
        x={picker.x}
        y={picker.y}
        onClose={() => setPicker((current) => ({ ...current, open: false }))}
        onSelect={(type, state) => {
          addWidget(type, picker.x, picker.y, state, getCanvasBounds());
          setPicker((current) => ({ ...current, open: false }));
        }}
      />
    </div>
  );
}
