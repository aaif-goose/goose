export const NAV_DIMENSIONS = {
  /** Width of condensed navigation in icon-only mode */
  CONDENSED_ICON_ONLY_WIDTH: 44,
  /** Width of condensed navigation with labels */
  CONDENSED_WIDTH: 200,
  /** Height of expanded navigation (horizontal mode) */
  EXPANDED_HEIGHT: 180,
  /** Height of condensed navigation (horizontal mode) */
  CONDENSED_HEIGHT: 46,
  /** Minimum width when resizing the navigation panel */
  MIN_NAV_WIDTH: 200,
  /** Maximum width when resizing the navigation panel */
  MAX_NAV_WIDTH: 600,
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
