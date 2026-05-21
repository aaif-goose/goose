import { Home, FileText, AppWindow, Clock, Puzzle, Settings, Zap } from 'lucide-react';
import type { LucideIcon } from 'lucide-react';
import { defineMessages, type IntlShape, type MessageDescriptor } from 'react-intl';

export interface NavItem {
  id: string;
  path: string;
  label: string;
  icon: LucideIcon;
  getTag?: () => string;
  tagAlign?: 'left' | 'right';
}

/** Top-level nav items (excluding Settings which is pinned to the bottom). */
export const NAV_ITEMS: NavItem[] = [
  { id: 'home', path: '/', label: 'Home', icon: Home },
  { id: 'recipes', path: '/recipes', label: 'Recipes', icon: FileText },
  { id: 'skills', path: '/skills', label: 'Skills', icon: Zap },
  { id: 'apps', path: '/apps', label: 'Apps', icon: AppWindow },
  { id: 'scheduler', path: '/schedules', label: 'Scheduler', icon: Clock },
  { id: 'extensions', path: '/extensions', label: 'Extensions', icon: Puzzle },
];

/** Settings is rendered separately, pinned to the bottom of the sidebar. */
export const SETTINGS_NAV_ITEM: NavItem = {
  id: 'settings',
  path: '/settings',
  label: 'Settings',
  icon: Settings,
};

// Translation descriptors for nav labels. Kept here next to NAV_ITEMS so the two
// stay in sync.
const navItemMessages = defineMessages({
  home: {
    id: 'navigation.itemHome',
    defaultMessage: 'Home',
  },
  recipes: {
    id: 'navigation.itemRecipes',
    defaultMessage: 'Recipes',
  },
  skills: {
    id: 'navigation.itemSkills',
    defaultMessage: 'Skills',
  },
  apps: {
    id: 'navigation.itemApps',
    defaultMessage: 'Apps',
  },
  scheduler: {
    id: 'navigation.itemScheduler',
    defaultMessage: 'Scheduler',
  },
  extensions: {
    id: 'navigation.itemExtensions',
    defaultMessage: 'Extensions',
  },
  settings: {
    id: 'navigation.itemSettings',
    defaultMessage: 'Settings',
  },
});

const NAV_ITEM_MESSAGES: Record<string, MessageDescriptor> = navItemMessages;

/** Format a NavItem's label using the provided intl instance, falling back to `item.label`. */
export function getNavItemLabel(item: NavItem, intl: IntlShape): string {
  const descriptor = NAV_ITEM_MESSAGES[item.id];
  return descriptor ? intl.formatMessage(descriptor) : item.label;
}
