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

export function getNavItemById(id: string): NavItem | undefined {
  return NAV_ITEMS.find((item) => item.id === id);
}
