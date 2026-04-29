import { AgentPinWidget } from "./AgentPinWidget";
import { ChatPinWidget } from "./ChatPinWidget";
import { ClockWidget } from "./ClockWidget";
import { CubeWidget } from "./CubeWidget";
import { MondayBriefTile } from "./MondayBriefTile";
import { StickyNoteWidget } from "./StickyNoteWidget";
import { WeatherWidget } from "./WeatherWidget";
import { WeeklyHighlightsTile } from "./WeeklyHighlightsTile";
import type { WidgetCatalogEntry } from "./types";

export const HOME_WIDGET_CATALOG = [
  {
    id: "mondayBrief",
    category: "tile",
    labelKey: "widgets.mondayBrief.label",
    descriptionKey: "widgets.mondayBrief.description",
    defaultSize: { width: 300, height: 180 },
    Component: MondayBriefTile,
  },
  {
    id: "weeklyHighlights",
    category: "tile",
    labelKey: "widgets.weeklyHighlights.label",
    descriptionKey: "widgets.weeklyHighlights.description",
    defaultSize: { width: 300, height: 200 },
    Component: WeeklyHighlightsTile,
  },
  {
    id: "cube",
    category: "app",
    labelKey: "widgets.cube.label",
    descriptionKey: "widgets.cube.description",
    defaultSize: { width: 360, height: 360 },
    Component: CubeWidget,
  },
  {
    id: "clock",
    category: "app",
    labelKey: "widgets.clock.label",
    descriptionKey: "widgets.clock.description",
    defaultSize: { width: 260, height: 132 },
    Component: ClockWidget,
  },
  {
    id: "weather",
    category: "app",
    labelKey: "widgets.weather.label",
    descriptionKey: "widgets.weather.description",
    defaultSize: { width: 280, height: 220 },
    Component: WeatherWidget,
  },
  {
    id: "stickyNote",
    category: "app",
    labelKey: "widgets.stickyNote.label",
    descriptionKey: "widgets.stickyNote.description",
    defaultSize: { width: 260, height: 220 },
    Component: StickyNoteWidget,
  },
  {
    id: "agentPin",
    category: "pin",
    labelKey: "widgets.agentPin.label",
    descriptionKey: "widgets.agentPin.description",
    defaultSize: { width: 136, height: 270 },
    Component: AgentPinWidget,
  },
  {
    id: "chatPin",
    category: "pin",
    labelKey: "widgets.chatPin.label",
    descriptionKey: "widgets.chatPin.description",
    defaultSize: { width: 260, height: 128 },
    Component: ChatPinWidget,
  },
] satisfies WidgetCatalogEntry[];

export const HOME_WIDGET_CATALOG_BY_ID = Object.fromEntries(
  HOME_WIDGET_CATALOG.map((entry) => [entry.id, entry]),
) as Record<string, WidgetCatalogEntry>;

export const HOME_WIDGET_CATEGORIES = ["tile", "app", "pin"] as const;
