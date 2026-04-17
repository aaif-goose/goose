---
title: Cash App Extension
description: Add Cash App as a goose Extension
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import GooseDesktopInstaller from '@site/src/components/GooseDesktopInstaller';
import CLIExtensionInstructions from '@site/src/components/CLIExtensionInstructions';

Cash App brings local food ordering into the AI era. Discover nearby sellers, browse menus with calories and allergen info, customize orders, and complete checkout—all conversationally. The real magic? Combine it with your other tools: check your calendar for meeting times, log meals to your health app, or find options that fit your dietary goals. Try prompts like "Order a nut-free lunch that arrives before my 2pm flight" for truly connected experiences.

## Quick Install

:::tip
<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
  [Install Cash App extension](goose://extension?type=streamable_http&url=https%3A%2F%2Fconnect.squareup.com%2Fv2%2Fmcp%2Fcash-app&id=cash-app&name=Cash%20App&description=Cash%20App%20brings%20local%20food%20ordering%20into%20the%20AI%20era.)
  </TabItem>
  <TabItem value="cli" label="goose CLI">
  Use `goose configure` to add a `Remote Extension (Streamable HTTP)` with:

  **Endpoint URL**

  ```
  https://connect.squareup.com/v2/mcp/cash-app
  ```
  </TabItem>
</Tabs>
:::

## Configuration

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    <GooseDesktopInstaller
      extensionId="cash-app"
      extensionName="Cash App"
      description="Cash App brings local food ordering into the AI era. Discover nearby sellers, browse menus, customize orders, and complete checkout—all conversationally."
      type="http"
      url="https://connect.squareup.com/v2/mcp/cash-app"
    />
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    <CLIExtensionInstructions
      name="Cash App"
      description="Cash App brings local food ordering into the AI era. Discover nearby sellers, browse menus, customize orders, and complete checkout—all conversationally."
      type="http"
      url="https://connect.squareup.com/v2/mcp/cash-app"
    />
  </TabItem>
</Tabs>

## What You Can Do

### Discover Nearby Restaurants

Find restaurants near any location. Filter by cuisine preference, brand name, whether they're currently open, or just describe what you're in the mood for.

**Prompt:**
```
Find me restaurants near 375 W Broadway in New York that are open now.
I'm in the mood for something quick and healthy.
```

<details>
<summary>goose Output</summary>

```
Here are restaurants near 375 W Broadway:

1. Ba'al Falafel - 71 Sullivan St — Mediterranean. Fresh falafel,
   hummus, and salads. (hours: 10:00 AM - 10:00 PM)
2. Kale & Things - 284 Lafayette St — Healthy bowls and salads
   with lean proteins. (hours: 9:00 AM - 9:00 PM)
3. Simply Natural Grill House NY - 684 Broadway — Grilled proteins
   and wholesome meals. (hours: 11:00 AM - 10:00 PM)
4. Steam & Sip - 112 Spring St — Coffee and light bites.
   (hours: 7:00 AM - 7:00 PM)

For a quick, healthy lunch I'd recommend Ba'al Falafel or Kale & Things —
both are close by and have great light options. Want me to show you
either menu?
```

</details>

### Browse Menus with Prices and Recommendations

Get a full menu overview with categories, prices, descriptions, and recommended items — all in plain text that goose can reason about.

**Prompt:**
```
Show me the full menu for Ba'al Falafel at 71 Sullivan St.
```

<details>
<summary>goose Output</summary>

```
# Ba'al Falafel

## Recommended
- **Classic Falafel Sandwich** — $9.99: Crispy falafel balls with tahini,
  pickles, and fresh vegetables in warm pita
- **Mediterranean Bowl** — $12.99: Falafel, hummus, tabbouleh, and mixed
  greens with tahini dressing

## Sandwiches
- **Classic Falafel Sandwich** — $9.99: Crispy falafel balls with tahini
- **Shawarma Wrap** — $11.99: Seasoned chicken shawarma with garlic sauce
- **Sabich** — $10.99: Fried eggplant, hard-boiled egg, and amba sauce

## Salads
- **Beets Apple Salad** — $11.99: Roasted beets, apple, walnuts, goat cheese
- **Fattoush Salad** — $10.99: Crispy pita chips with fresh vegetables

## Soups
- **Lentil Soup** — $6.99: Traditional red lentil soup with cumin

## Drinks
- **Ginger Lemonade** — $4.99: Fresh-squeezed with ginger
- **Mint Tea** — $3.99: Fresh mint leaves steeped in hot water
```

</details>

Because the menu comes back as structured text, goose can answer follow-up questions like *"which items are under $10?"* or *"what's vegetarian?"* without another tool call.

### Build a Cart and Check Out

Once you've found what you want, tell goose to build your order. It handles item selection, modifiers, and quantities — then gives you a checkout link to complete payment through Cash App.

**Prompt:**
```
Add a Classic Falafel Sandwich and a Ginger Lemonade to my cart.
```

## Example Prompts

Cash App is most powerful when goose combines it with your other tools and the context of your day:

- **Time-aware ordering:** *"Order a nut-free lunch that arrives before my 2pm flight."*
- **Pair with your calendar:** *"Check my calendar for today and find somewhere I can grab a quick pickup lunch before my 1pm meeting. I'm near Union Square."*
- **Health-conscious choices:** *"Find restaurants near me with salads under 500 calories and show me the options."*
- **Plan for a group:** *"I'm meeting 3 friends near Bryant Park tonight. Find somewhere with good vegetarian options and show me the menu."*
- **Explore a new neighborhood:** *"I just moved to Williamsburg. What are the best restaurants within walking distance of Bedford Ave?"*
- **Budget-conscious ordering:** *"Find lunch specials under $12 near Times Square."*
