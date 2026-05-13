import {
  Home,
  MessageSquare,
  FileText,
  AppWindow,
  Clock,
  Puzzle,
  Settings,
  Zap,
} from 'lucide-react';
import type { LucideIcon } from 'lucide-react';
import { defineMessages, type IntlShape, type MessageDescriptor } from 'react-intl';

export interface NavItem {
  id: string;
  path: string;
  label: string;
  icon: LucideIcon;
  getTag?: () => string;
  tagAlign?: 'left' | 'right';
  hasSubItems?: boolean;
}

export const NAV_ITEMS: NavItem[] = [
  { id: 'home', path: '/', label: 'Home', icon: Home },
  { id: 'chat', path: '/pair', label: 'Chat', icon: MessageSquare, hasSubItems: true },
  { id: 'recipes', path: '/recipes', label: 'Recipes', icon: FileText },
  { id: 'skills', path: '/skills', label: 'Skills', icon: Zap },
  { id: 'apps', path: '/apps', label: 'Apps', icon: AppWindow },
  { id: 'scheduler', path: '/schedules', label: 'Scheduler', icon: Clock },
  { id: 'extensions', path: '/extensions', label: 'Extensions', icon: Puzzle },
  { id: 'settings', path: '/settings', label: 'Settings', icon: Settings },
];

// Translation descriptors for nav labels. Kept here next to NAV_ITEMS so the two
// stay in sync. Reuses the existing `navigationCustomization.item*` ids that are
// also used by the "Customize Navigation" settings screen.
const navItemMessages = defineMessages({
  home: {
    id: 'navigationCustomization.itemHome',
    defaultMessage: 'Home',
  },
  chat: {
    id: 'navigationCustomization.itemChat',
    defaultMessage: 'Chat',
  },
  recipes: {
    id: 'navigationCustomization.itemRecipes',
    defaultMessage: 'Recipes',
  },
  skills: {
    id: 'navigationCustomization.itemSkills',
    defaultMessage: 'Skills',
  },
  apps: {
    id: 'navigationCustomization.itemApps',
    defaultMessage: 'Apps',
  },
  scheduler: {
    id: 'navigationCustomization.itemScheduler',
    defaultMessage: 'Scheduler',
  },
  extensions: {
    id: 'navigationCustomization.itemExtensions',
    defaultMessage: 'Extensions',
  },
  settings: {
    id: 'navigationCustomization.itemSettings',
    defaultMessage: 'Settings',
  },
});

const NAV_ITEM_MESSAGES: Record<string, MessageDescriptor> = navItemMessages;

/** Format a NavItem's label using the provided intl instance, falling back to `item.label`. */
export function getNavItemLabel(item: NavItem, intl: IntlShape): string {
  const descriptor = NAV_ITEM_MESSAGES[item.id];
  return descriptor ? intl.formatMessage(descriptor) : item.label;
}

export function getNavItemById(id: string): NavItem | undefined {
  return NAV_ITEMS.find((item) => item.id === id);
}
