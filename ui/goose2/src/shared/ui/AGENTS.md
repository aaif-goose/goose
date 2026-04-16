# Shared UI Design System

Use the primitives in `src/shared/ui/` as the default way to build interface elements. Prefer extending these components over recreating their styling in feature code.

## Core Rules

- Prefer shared UI primitives over custom markup plus Tailwind classes.
- Prefer adding a variant or prop to a shared component over one-off styling in feature code.
- Keep feature code focused on composition, state, and content. Visual treatment should usually live in `src/shared/ui/`.
- If a pattern appears in more than one place, extract or extend a shared primitive instead of copying class strings.

## Buttons

- Use `Button` for clickable controls unless there is a strong reason not to.
- Use `variant` and `size` before adding custom classes.
- Use `leftIcon` and `rightIcon` for leading and trailing icons instead of manually placing icon children.
- Do not add spacing classes only to position button icons unless the design system cannot express the pattern yet.
- For icon-only actions, use the `icon-*` sizes instead of text button sizes.
- For active icon-only buttons, prefer native hover via `title` plus `aria-label`.
- Reserve custom `Tooltip` for disabled controls or richer explanatory content.

### Ghost icon buttons

`variant="ghost"` has compound variants for all `icon-*` sizes that set `hover:bg-transparent hover:text-foreground`. This means ghost icon buttons have no background fill on hover — only a color change. Do not add `hover:bg-accent/50` or similar hover background classes to ghost icon buttons; the compound variant already provides the correct behavior. Layout classes like `mr-1`, `size-6`, `flex-shrink-0` are fine to add.

## Icon Sizing

- Let `Button` size button icons by default.
- Current button icon defaults are tied to button size:
- `xs` and `sm` buttons use `size-3` icons.
- `default` buttons use `size-3.5` icons.
- `lg` buttons use `size-4` icons.
- Only give an icon its own explicit `size-*` class when intentionally overriding the design-system default.
- Match icon visual weight to the text and control size. Small toolbar controls should not use oversized icons.

## Menus And Selectors

- Compose menus from `DropdownMenu`, `DropdownMenuTrigger`, `DropdownMenuContent`, and related shared primitives.
- When a trigger behaves like a button, use `Button` as the trigger surface.
- Repeated trigger patterns like icon + label + chevron should be expressed through shared props or shared wrapper components.

## Styling Boundaries

- Avoid introducing custom colors, spacing, radii, or typography in feature code when existing tokens and shared variants cover the need.
- If a control needs a new visual treatment, add it to the shared component API first.
- Keep accessibility built in: semantic elements, labels for icon-only buttons, and consistent focus states.

## Good Heuristic

Before writing custom classes in a feature, ask:

1. Can an existing shared component already do this?
2. Should this become a shared variant or prop?
3. Will another screen likely need the same pattern?

