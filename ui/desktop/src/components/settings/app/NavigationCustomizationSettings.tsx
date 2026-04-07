import React, { useState } from 'react';
import { GripVertical, Eye, EyeOff } from 'lucide-react';
import { defineMessages, useIntl } from '../../../i18n';
import {
  useNavigationContext,
  DEFAULT_ITEM_ORDER,
  DEFAULT_ENABLED_ITEMS,
} from '../../Layout/NavigationContext';
import { cn } from '../../../utils';

const i18n = defineMessages({
  dragInstructions: {
    id: 'navigationCustomization.dragInstructions',
    defaultMessage: 'Drag to reorder, click the eye icon to show/hide items',
  },
  resetToDefaults: {
    id: 'navigationCustomization.resetToDefaults',
    defaultMessage: 'Reset to defaults',
  },
  hideItem: {
    id: 'navigationCustomization.hideItem',
    defaultMessage: 'Hide item',
  },
  showItem: {
    id: 'navigationCustomization.showItem',
    defaultMessage: 'Show item',
  },
  itemHome: {
    id: 'navigationCustomization.itemHome',
    defaultMessage: 'Home',
  },
  itemChat: {
    id: 'navigationCustomization.itemChat',
    defaultMessage: 'Chat',
  },
  itemRecipes: {
    id: 'navigationCustomization.itemRecipes',
    defaultMessage: 'Recipes',
  },
  itemApps: {
    id: 'navigationCustomization.itemApps',
    defaultMessage: 'Apps',
  },
  itemScheduler: {
    id: 'navigationCustomization.itemScheduler',
    defaultMessage: 'Scheduler',
  },
  itemExtensions: {
    id: 'navigationCustomization.itemExtensions',
    defaultMessage: 'Extensions',
  },
  itemSettings: {
    id: 'navigationCustomization.itemSettings',
    defaultMessage: 'Settings',
  },
});

const ITEM_LABEL_KEYS: Record<string, keyof typeof i18n> = {
  home: 'itemHome',
  chat: 'itemChat',
  recipes: 'itemRecipes',
  apps: 'itemApps',
  scheduler: 'itemScheduler',
  extensions: 'itemExtensions',
  settings: 'itemSettings',
};

interface NavigationCustomizationSettingsProps {
  className?: string;
}

export const NavigationCustomizationSettings: React.FC<NavigationCustomizationSettingsProps> = ({
  className,
}) => {
  const { preferences, updatePreferences } = useNavigationContext();
  const [draggedItem, setDraggedItem] = useState<string | null>(null);
  const [dragOverItem, setDragOverItem] = useState<string | null>(null);
  const intl = useIntl();

  const handleDragStart = (e: React.DragEvent, itemId: string) => {
    setDraggedItem(itemId);
    e.dataTransfer.effectAllowed = 'move';
  };

  const handleDragOver = (e: React.DragEvent, itemId: string) => {
    e.preventDefault();
    if (draggedItem && draggedItem !== itemId) {
      setDragOverItem(itemId);
    }
  };

  const handleDrop = (e: React.DragEvent, dropItemId: string) => {
    e.preventDefault();
    if (!draggedItem || draggedItem === dropItemId) return;

    const newOrder = [...preferences.itemOrder];
    const draggedIndex = newOrder.indexOf(draggedItem);
    const dropIndex = newOrder.indexOf(dropItemId);

    if (draggedIndex === -1 || dropIndex === -1) return;

    newOrder.splice(draggedIndex, 1);
    newOrder.splice(dropIndex, 0, draggedItem);

    updatePreferences({
      ...preferences,
      itemOrder: newOrder,
    });

    setDraggedItem(null);
    setDragOverItem(null);
  };

  const handleDragEnd = () => {
    setDraggedItem(null);
    setDragOverItem(null);
  };

  const toggleItemEnabled = (itemId: string) => {
    const newEnabledItems = preferences.enabledItems.includes(itemId)
      ? preferences.enabledItems.filter((id) => id !== itemId)
      : [...preferences.enabledItems, itemId];

    updatePreferences({
      ...preferences,
      enabledItems: newEnabledItems,
    });
  };

  const resetToDefaults = () => {
    updatePreferences({
      itemOrder: DEFAULT_ITEM_ORDER,
      enabledItems: DEFAULT_ENABLED_ITEMS,
    });
  };

  const getItemLabel = (itemId: string): string => {
    const key = ITEM_LABEL_KEYS[itemId];
    if (key) {
      return intl.formatMessage(i18n[key]);
    }
    return itemId;
  };

  return (
    <div className={className}>
      <div className="space-y-3">
        <div className="flex items-center justify-between mb-4">
          <p className="text-sm text-text-secondary">
            {intl.formatMessage(i18n.dragInstructions)}
          </p>
          <button
            onClick={resetToDefaults}
            className="text-xs text-text-secondary hover:text-text-primary transition-colors"
          >
            {intl.formatMessage(i18n.resetToDefaults)}
          </button>
        </div>

        {preferences.itemOrder.map((itemId) => {
          const isEnabled = preferences.enabledItems.includes(itemId);
          const isDragging = draggedItem === itemId;
          const isDragOver = dragOverItem === itemId;
          const label = getItemLabel(itemId);

          return (
            <div
              key={itemId}
              draggable
              onDragStart={(e) => handleDragStart(e, itemId)}
              onDragOver={(e) => handleDragOver(e, itemId)}
              onDrop={(e) => handleDrop(e, itemId)}
              onDragEnd={handleDragEnd}
              className={cn(
                'flex items-center gap-3 p-3 rounded-lg border transition-all',
                isDragging && 'opacity-50',
                isDragOver
                  ? 'border-border-primary bg-background-tertiary'
                  : 'border-border-secondary bg-background-primary',
                !isEnabled && 'opacity-50'
              )}
            >
              <GripVertical className="w-4 h-4 text-text-secondary cursor-move flex-shrink-0" />
              <span className="flex-1 text-sm text-text-primary">{label}</span>
              <button
                onClick={() => toggleItemEnabled(itemId)}
                className="p-1 rounded hover:bg-background-tertiary transition-colors flex-shrink-0"
                title={isEnabled ? intl.formatMessage(i18n.hideItem) : intl.formatMessage(i18n.showItem)}
              >
                {isEnabled ? (
                  <Eye className="w-4 h-4 text-text-primary" />
                ) : (
                  <EyeOff className="w-4 h-4 text-text-secondary" />
                )}
              </button>
            </div>
          );
        })}
      </div>
    </div>
  );
};
