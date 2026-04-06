import { Marked } from "marked";
import { markedTerminal } from "marked-terminal";

let renderer: Marked | null = null;
let rendererWidth = 0;

function getRenderer(width: number): Marked {
  if (renderer && rendererWidth === width) return renderer;
  renderer = new Marked();
  renderer.use(markedTerminal({ width, reflowText: true, tab: 2 }) as any);
  rendererWidth = width;
  return renderer;
}

export function renderMarkdown(src: string, width = 76): string[] {
  if (!src) return [];
  const m = getRenderer(width);
  const rendered = (m.parse(src) as string).replace(/\n+$/, "");
  return rendered.split("\n");
}
