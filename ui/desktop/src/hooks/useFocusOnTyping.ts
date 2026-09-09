import { RefObject, useEffect } from 'react';

function isEditableElement(element: Element | null): boolean {
  if (!element) return false;
  if (
    element instanceof HTMLInputElement ||
    element instanceof HTMLTextAreaElement ||
    element instanceof HTMLSelectElement
  ) {
    return true;
  }
  return element instanceof HTMLElement && element.isContentEditable;
}

const SPACE_ACTIVATABLE_INPUT_TYPES = new Set([
  'checkbox',
  'radio',
  'submit',
  'button',
  'reset',
  'file',
  'range',
  'color',
]);

const SPACE_ACTIVATABLE_ROLES = new Set([
  'button',
  'checkbox',
  'radio',
  'switch',
  'menuitem',
  'menuitemcheckbox',
  'menuitemradio',
  'tab',
  'link',
]);

function isSpaceActivatableControl(element: Element | null): boolean {
  if (!(element instanceof HTMLElement)) return false;
  const tag = element.tagName;
  if (tag === 'BUTTON' || tag === 'A' || tag === 'SUMMARY') return true;
  if (tag === 'INPUT') {
    return SPACE_ACTIVATABLE_INPUT_TYPES.has((element as HTMLInputElement).type);
  }
  const role = element.getAttribute('role');
  return role !== null && SPACE_ACTIVATABLE_ROLES.has(role);
}

const COMPOSITE_WIDGET_SELECTOR =
  '[role="menu"], [role="listbox"], [role="tree"], [role="grid"], [role="combobox"], [data-radix-collection-item]';

function isInsideCompositeWidget(element: Element | null): boolean {
  return element instanceof HTMLElement && element.closest(COMPOSITE_WIDGET_SELECTOR) !== null;
}

export function useFocusOnTyping(
  targetRef: RefObject<HTMLTextAreaElement | null>,
  enabled: boolean
) {
  useEffect(() => {
    if (!enabled) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.defaultPrevented) return;
      if (e.metaKey || e.ctrlKey) return;
      if (e.key.length !== 1) return;
      if (isEditableElement(document.activeElement)) return;
      if (isInsideCompositeWidget(document.activeElement)) return;
      if (e.key === ' ' && isSpaceActivatableControl(document.activeElement)) return;

      const selection = window.getSelection();
      if (selection && !selection.isCollapsed) return;

      targetRef.current?.focus();
    };

    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, [targetRef, enabled]);
}
