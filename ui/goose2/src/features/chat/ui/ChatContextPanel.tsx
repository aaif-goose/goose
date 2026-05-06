import {
  IconLayoutSidebarRight,
  IconLayoutSidebarRightFilled,
} from "@tabler/icons-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useEffect, useState } from "react";
import { Button } from "@/shared/ui/button";
import { cn } from "@/shared/lib/cn";
import { ContextPanel } from "./ContextPanel";

const CP_PAD = 12;
const CP_TOTAL_W = 340 + CP_PAD * 2;
const CP_TOGGLE_RIGHT = CP_PAD + 12;
const CP_TOGGLE_TOP = CP_PAD + 10;
const CP_FADE_S = 0.15;
const CP_REFLOW_MS = 200;
const CP_COMPACT_QUERY = "(max-width: 900px)";

interface ChatContextPanelProps {
  activeSessionId: string;
  isOpen: boolean;
  label: string;
  project?: {
    name?: string;
    color?: string;
    workingDirs?: string[];
  } | null;
  setOpen: (sessionId: string, open: boolean) => void;
}

export function ChatContextPanel({
  activeSessionId,
  isOpen,
  label,
  project,
  setOpen,
}: ChatContextPanelProps) {
  const shouldReduceMotion = useReducedMotion();
  const [isCompactViewport, setIsCompactViewport] = useState(false);
  const fadeTransition = { duration: shouldReduceMotion ? 0 : CP_FADE_S };
  const reflowDuration = shouldReduceMotion ? 0 : CP_REFLOW_MS;

  useEffect(() => {
    if (!window.matchMedia) return;

    const mediaQuery = window.matchMedia(CP_COMPACT_QUERY);
    setIsCompactViewport(mediaQuery.matches);

    const handleChange = (event: MediaQueryListEvent) => {
      setIsCompactViewport(event.matches);
    };

    mediaQuery.addEventListener("change", handleChange);
    return () => mediaQuery.removeEventListener("change", handleChange);
  }, []);

  return (
    <>
      <div
        className={cn(
          "shrink-0",
          isCompactViewport ? "overflow-visible" : "overflow-hidden",
        )}
        style={{
          width: isOpen && !isCompactViewport ? CP_TOTAL_W : 0,
          transition: `width ${reflowDuration}ms ease`,
        }}
      >
        <AnimatePresence initial={false}>
          {isOpen ? (
            <motion.div
              key="context-panel"
              className={cn(
                "flex",
                isCompactViewport
                  ? "absolute bottom-3 right-3 top-12 z-10 w-[min(340px,calc(100%-1.5rem))]"
                  : "h-full",
              )}
              style={
                isCompactViewport
                  ? undefined
                  : {
                      width: CP_TOTAL_W,
                      padding: CP_PAD,
                    }
              }
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={fadeTransition}
            >
              <aside
                className={cn(
                  "flex min-w-0 flex-1 overflow-hidden rounded-xl border border-border bg-background",
                  isCompactViewport && "shadow-modal",
                )}
              >
                <ContextPanel
                  sessionId={activeSessionId}
                  projectName={project?.name}
                  projectColor={project?.color}
                  projectWorkingDirs={project?.workingDirs ?? []}
                />
              </aside>
            </motion.div>
          ) : null}
        </AnimatePresence>
      </div>

      <div
        className="absolute z-20"
        style={{
          right: CP_TOGGLE_RIGHT,
          top: CP_TOGGLE_TOP,
        }}
      >
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          onClick={() => setOpen(activeSessionId, !isOpen)}
          className={
            isOpen
              ? "text-muted-foreground transition-opacity duration-150 hover:text-foreground"
              : "h-9 w-11 rounded-sm border border-border bg-background/80 text-muted-foreground shadow-none backdrop-blur-sm transition-opacity duration-150 hover:bg-accent/50 hover:text-foreground"
          }
          aria-label={label}
          title={label}
        >
          {isOpen ? (
            <IconLayoutSidebarRightFilled className="size-4" />
          ) : (
            <IconLayoutSidebarRight className="size-4" />
          )}
        </Button>
      </div>
    </>
  );
}
