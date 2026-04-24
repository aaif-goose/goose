import {
  IconLayoutSidebarRight,
  IconLayoutSidebarRightFilled,
} from "@tabler/icons-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { Button } from "@/shared/ui/button";
import { ContextPanel } from "./ContextPanel";

const CP_PAD = 12;
const CP_TOTAL_W = 340 + CP_PAD * 2;
const CP_TOGGLE_RIGHT = CP_PAD + 12;
const CP_TOGGLE_TOP = CP_PAD + 10;
const CP_FADE_S = 0.15;
const CP_REFLOW_MS = 200;

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
  const fadeTransition = { duration: shouldReduceMotion ? 0 : CP_FADE_S };
  const reflowDuration = shouldReduceMotion ? 0 : CP_REFLOW_MS;

  return (
    <>
      <div
        className="shrink-0 overflow-hidden"
        style={{
          width: isOpen ? CP_TOTAL_W : 0,
          transition: `width ${reflowDuration}ms ease`,
        }}
      >
        <AnimatePresence initial={false}>
          {isOpen ? (
            <motion.div
              key="context-panel"
              className="flex h-full"
              style={{
                width: CP_TOTAL_W,
                padding: CP_PAD,
              }}
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={fadeTransition}
            >
              <aside className="flex min-w-0 flex-1 overflow-hidden rounded-xl border border-border bg-background">
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
