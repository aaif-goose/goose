import type { ComponentType } from "react";

export type WidgetCategory = "tile" | "app" | "pin";

export interface WidgetSize {
  width: number;
  height: number;
}

export interface WidgetInstance {
  id: string;
  type: string;
  x: number;
  y: number;
  z: number;
  state?: Record<string, unknown>;
}

export interface WidgetNavigationHandlers {
  onOpenAgent?: (agentId: string) => void;
  onStartChatWithPersona?: (personaId: string) => void;
  onSelectSession?: (sessionId: string) => void;
}

export interface WidgetRenderProps extends WidgetNavigationHandlers {
  instance: WidgetInstance;
  onUpdateState: (next: Record<string, unknown>) => void;
  shouldIgnoreActivation: () => boolean;
}

export interface WidgetCatalogEntry {
  id: string;
  category: WidgetCategory;
  labelKey: string;
  descriptionKey?: string;
  defaultSize: WidgetSize;
  Component: ComponentType<WidgetRenderProps>;
}

export interface CanvasBounds {
  width: number;
  height: number;
}
