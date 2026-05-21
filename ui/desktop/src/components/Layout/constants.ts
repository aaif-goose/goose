export const NAV_DIMENSIONS = {
  /** Width of the navigation sidebar */
  NAV_WIDTH: 240,
} as const;

export const Z_INDEX = {
  /** Header controls (menu button, etc.) */
  HEADER: 100,
  /** Tooltips - should appear above most UI elements */
  TOOLTIP: 200,
  /** Popover content (hover menus) */
  POPOVER: 9999,
  /** Modal/overlay backdrop and content */
  OVERLAY: 10000,
  /** Dropdown menus that appear above overlays */
  DROPDOWN_ABOVE_OVERLAY: 10001,
} as const;
