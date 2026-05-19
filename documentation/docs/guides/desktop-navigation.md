---
title: Customizing the Sidebar
sidebar_label: Customizing the Sidebar
description: Personalize how you navigate goose Desktop by changing the sidebar's look, position, and behavior
---

import { PanelLeft, Menu } from 'lucide-react';

:::info Desktop Only
Sidebar customization is available only in goose Desktop.
:::

The sidebar is how you navigate between pages in goose Desktop: Home, Chat, Recipes, Apps, Scheduler, Extensions, and Settings. You can customize its appearance, position, and behavior to fit the way you work.

To access these settings:

1. Click the <PanelLeft className="inline" size={16} /> button in the top-left to open the sidebar
2. Click `Settings` in the sidebar
3. Click the `App` tab
4. Scroll to the **Navigation** section

## Style

Choose how the sidebar displays its items.

| Style | Description |
|-------|-------------|
| **Tile** (default) | Large icons with labels arranged in a grid. Good for quick visual scanning. |
| **List** | A compact, single-column list. Takes up less space and shows more items at once. |

:::note
In List style on smaller windows (under 700px wide), the sidebar automatically collapses to icons only.
:::

## Position

Move the sidebar to any edge of the window.

| Position | Description |
|----------|-------------|
| **Left** (default) | Vertical sidebar on the left side of the window |
| **Right** | Vertical sidebar on the right side of the window |
| **Top** | Horizontal bar along the top of the window |
| **Bottom** | Horizontal bar along the bottom of the window |

## Mode

Control what happens to your content when the sidebar is open.

| Mode | Description |
|------|-------------|
| **Push** (default) | The sidebar takes up space and your content shifts to make room. The sidebar stays visible while you work. |
| **Overlay** | The sidebar floats on top of your content without moving anything. Click outside or navigate to dismiss it. |

:::note
In Overlay mode, the Style and Position settings are not available. The sidebar always appears as a full-screen tile overlay.
:::

## Customize Items

You can reorder and hide items in the sidebar from the **Customize Items** section of the navigation settings.

- **Drag items** to reorder them
- Click the **eye icon** to show or hide individual items

Click `Reset to defaults` to restore the original order and visibility.

## Toggle Sidebar

Show or hide the sidebar without opening Settings:

- **Menu bar:** Go to **View → Toggle Navigation**
- **Keyboard shortcut:** The shortcut is shown next to the menu item and can be customized in `Settings` > `Keyboard`

The sidebar's open/closed state is remembered across sessions.
