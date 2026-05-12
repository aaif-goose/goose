import { useEffect, type RefObject } from "react";

type ColorScheme = "light" | "dark";

export function useIframeColorScheme<T extends HTMLElement>(
  rootRef: RefObject<T | null>,
  colorScheme: ColorScheme,
) {
  useEffect(() => {
    const root = rootRef.current;
    if (!root) {
      return;
    }

    const applyColorScheme = () => {
      for (const iframe of root.querySelectorAll("iframe")) {
        iframe.style.setProperty("color-scheme", colorScheme);
        iframe.style.backgroundColor = "transparent";
      }
    };

    applyColorScheme();

    if (typeof MutationObserver === "undefined") {
      return;
    }

    const observer = new MutationObserver(applyColorScheme);
    observer.observe(root, { childList: true, subtree: true });
    return () => {
      observer.disconnect();
    };
  }, [rootRef, colorScheme]);
}
