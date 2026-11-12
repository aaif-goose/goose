import type { McpUiHostContext } from '@modelcontextprotocol/ext-apps/app-bridge';
import type { DimensionLayout, GooseDisplayMode } from './types';

export const DISPLAY_MODE_LAYOUTS: Record<GooseDisplayMode, DimensionLayout> = {
  inline: { width: 'fixed', height: 'unbounded' },
  fullscreen: { width: 'fixed', height: 'fixed' },
  standalone: { width: 'fixed', height: 'fixed' },
  pip: { width: 'fixed', height: 'flexible' },
};

export function getContainerDimensions(
  displayMode: GooseDisplayMode,
  measuredWidth: number,
  measuredHeight: number
): McpUiHostContext['containerDimensions'] {
  const layout = DISPLAY_MODE_LAYOUTS[displayMode] ?? DISPLAY_MODE_LAYOUTS.inline;

  // Only require a measurement for axes that are fixed or flexible (unbounded axes are omitted).
  if (
    (layout.width !== 'unbounded' && measuredWidth <= 0) ||
    (layout.height !== 'unbounded' && measuredHeight <= 0)
  )
    return undefined;

  const widthDimension = (() => {
    switch (layout.width) {
      case 'fixed':
        return { width: measuredWidth };
      case 'flexible':
        return { maxWidth: measuredWidth };
      case 'unbounded':
        return {};
    }
  })();

  const heightDimension = (() => {
    switch (layout.height) {
      case 'fixed':
        return { height: measuredHeight };
      case 'flexible':
        return { maxHeight: measuredHeight };
      case 'unbounded':
        return {};
    }
  })();

  return { ...widthDimension, ...heightDimension };
}
