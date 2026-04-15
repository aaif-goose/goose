import { useEffect } from "react";

const SCROLL_FADE_MS = 1200;
const STYLE_ID = "scroll-fade-styles";

const THUMB_HIDDEN = "transparent";
// WebKit doesn't reliably resolve CSS variables inside ::-webkit-scrollbar-thumb,
// so we use hardcoded rgba values that approximate --muted-foreground at reduced opacity.
const THUMB_VISIBLE = "rgba(150, 150, 150, 0.4)";
const THUMB_HOVER = "rgba(150, 150, 150, 0.6)";

function buildCSS(thumb: string, thumbHover: string) {
  return `
::-webkit-scrollbar-thumb { background: ${thumb} !important; }
::-webkit-scrollbar-thumb:hover { background: ${thumbHover} !important; }
`;
}

function findScrollableAncestor(el: Element | null): Element | null {
  while (el) {
    if (el.scrollHeight > el.clientHeight || el.scrollWidth > el.clientWidth) {
      const style = getComputedStyle(el);
      if (
        style.overflowY === "auto" ||
        style.overflowY === "scroll" ||
        style.overflowX === "auto" ||
        style.overflowX === "scroll"
      ) {
        return el;
      }
    }
    el = el.parentElement;
  }
  return null;
}

export function useScrollFade() {
  useEffect(() => {
    const existing = document.getElementById(STYLE_ID);
    if (existing) existing.remove();

    const style = document.createElement("style");
    style.id = STYLE_ID;
    style.textContent = buildCSS(THUMB_HIDDEN, THUMB_HIDDEN);
    document.head.appendChild(style);

    let timer: ReturnType<typeof setTimeout> | undefined;
    let visible = false;

    function showScrollbar() {
      if (visible) return;
      visible = true;
      style.textContent = buildCSS(THUMB_VISIBLE, THUMB_HOVER);
    }

    function hideScrollbar() {
      visible = false;
      style.textContent = buildCSS(THUMB_HIDDEN, THUMB_HIDDEN);
    }

    function onActivity() {
      showScrollbar();
      clearTimeout(timer);
      timer = setTimeout(hideScrollbar, SCROLL_FADE_MS);
    }

    const handleScroll = (e: Event) => {
      if (e.target instanceof Element) onActivity();
    };

    const handleWheel = (e: WheelEvent) => {
      if (findScrollableAncestor(e.target as Element)) onActivity();
    };

    document.addEventListener("scroll", handleScroll, true);
    document.addEventListener("wheel", handleWheel, { passive: true });
    return () => {
      document.removeEventListener("scroll", handleScroll, true);
      document.removeEventListener("wheel", handleWheel);
      clearTimeout(timer);
      style.remove();
    };
  }, []);
}
