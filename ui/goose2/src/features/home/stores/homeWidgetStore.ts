import { create } from "zustand";
import { persist } from "zustand/middleware";
import { HOME_WIDGET_CATALOG_BY_ID } from "../widgets/catalog";
import type { CanvasBounds, WidgetInstance } from "../widgets/types";

export const HOME_WIDGET_STORAGE_KEY = "goose2:home-widgets";

const BASELINE_CANVAS: CanvasBounds = { width: 1080, height: 760 };

function maxZ(instances: WidgetInstance[]): number {
  return instances.reduce((max, instance) => Math.max(max, instance.z), 0);
}

export function clampWidgetPosition(
  type: string,
  x: number,
  y: number,
  bounds?: CanvasBounds,
): { x: number; y: number } {
  const catalogEntry = HOME_WIDGET_CATALOG_BY_ID[type];
  if (!catalogEntry || !bounds) {
    return { x, y };
  }

  return {
    x: Math.min(
      Math.max(0, x),
      Math.max(0, bounds.width - catalogEntry.defaultSize.width),
    ),
    y: Math.min(
      Math.max(0, y),
      Math.max(0, bounds.height - catalogEntry.defaultSize.height),
    ),
  };
}

function positionFromAnchor(
  type: string,
  anchorX: number,
  anchorY: number,
  bounds = BASELINE_CANVAS,
): { x: number; y: number } {
  const size = HOME_WIDGET_CATALOG_BY_ID[type]?.defaultSize ?? {
    width: 0,
    height: 0,
  };
  return clampWidgetPosition(
    type,
    bounds.width * anchorX - size.width / 2,
    bounds.height * anchorY - size.height / 2,
    bounds,
  );
}

export function createDefaultHomeWidgets(
  bounds = BASELINE_CANVAS,
): WidgetInstance[] {
  const cube = positionFromAnchor("cube", 0.5, 0.48, bounds);
  const clock = positionFromAnchor("clock", 0.83, 0.18, bounds);
  const agentPin = positionFromAnchor("agentPin", 0.2, 0.78, bounds);

  return [
    { id: "default-cube", type: "cube", x: cube.x, y: cube.y, z: 1 },
    { id: "default-clock", type: "clock", x: clock.x, y: clock.y, z: 2 },
    {
      id: "default-agent-pin",
      type: "agentPin",
      x: agentPin.x,
      y: agentPin.y,
      z: 3,
      state: { agentId: "scout" },
    },
  ];
}

interface HomeWidgetStore {
  instances: WidgetInstance[];
  addWidget: (
    type: string,
    x: number,
    y: number,
    state?: Record<string, unknown>,
    bounds?: CanvasBounds,
  ) => void;
  moveWidget: (id: string, x: number, y: number, bounds?: CanvasBounds) => void;
  bumpZ: (id: string) => void;
  removeWidget: (id: string) => void;
  updateWidgetState: (id: string, state: Record<string, unknown>) => void;
}

export const useHomeWidgetStore = create<HomeWidgetStore>()(
  persist(
    (set) => ({
      instances: createDefaultHomeWidgets(),
      addWidget: (type, x, y, state, bounds) =>
        set((current) => {
          const catalogEntry = HOME_WIDGET_CATALOG_BY_ID[type];
          if (!catalogEntry) {
            return current;
          }

          const centered = clampWidgetPosition(
            type,
            x - catalogEntry.defaultSize.width / 2,
            y - catalogEntry.defaultSize.height / 2,
            bounds,
          );

          return {
            instances: [
              ...current.instances,
              {
                id: crypto.randomUUID(),
                type,
                x: centered.x,
                y: centered.y,
                z: maxZ(current.instances) + 1,
                state,
              },
            ],
          };
        }),
      moveWidget: (id, x, y, bounds) =>
        set((current) => ({
          instances: current.instances.map((instance) =>
            instance.id === id
              ? {
                  ...instance,
                  ...clampWidgetPosition(instance.type, x, y, bounds),
                }
              : instance,
          ),
        })),
      bumpZ: (id) =>
        set((current) => {
          const nextZ = maxZ(current.instances) + 1;
          return {
            instances: current.instances.map((instance) =>
              instance.id === id ? { ...instance, z: nextZ } : instance,
            ),
          };
        }),
      removeWidget: (id) =>
        set((current) => ({
          instances: current.instances.filter((instance) => instance.id !== id),
        })),
      updateWidgetState: (id, state) =>
        set((current) => ({
          instances: current.instances.map((instance) =>
            instance.id === id
              ? {
                  ...instance,
                  state: { ...(instance.state ?? {}), ...state },
                }
              : instance,
          ),
        })),
    }),
    {
      name: HOME_WIDGET_STORAGE_KEY,
      version: 1,
      partialize: (state) => ({ instances: state.instances }),
    },
  ),
);
