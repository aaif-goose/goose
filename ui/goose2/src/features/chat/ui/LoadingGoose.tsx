import { useTranslation } from "react-i18next";
import { motion, useReducedMotion } from "motion/react";
import { Shimmer } from "@/shared/ui/ai-elements/shimmer";

export type LoadingChatState =
  | "idle"
  | "thinking"
  | "streaming"
  | "waiting"
  | "compacting";

interface LoadingGooseProps {
  chatState?: LoadingChatState;
}

const LOADING_FADE_S = 0.45;
const LOADING_SHIMMER_S = 3;
const LOADING_SHIMMER_SPREAD = 3;
const LOADING_SHIMMER_DELAY_S = 0.35;
const LOADING_SHIMMER_REPEAT_DELAY_S = 0.9;

const MESSAGE_KEY_BY_STATE: Record<
  Exclude<LoadingChatState, "idle">,
  "thinking" | "responding"
> = {
  thinking: "thinking",
  streaming: "responding",
  waiting: "responding",
  compacting: "responding",
};

export function LoadingGoose({ chatState = "idle" }: LoadingGooseProps) {
  const { t } = useTranslation("chat");
  const shouldReduceMotion = useReducedMotion();
  if (chatState === "idle") {
    return null;
  }

  const message = t(`loading.${MESSAGE_KEY_BY_STATE[chatState]}`);

  return (
    <motion.div
      className="px-4"
      role="status"
      aria-label={message}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: shouldReduceMotion ? 0 : LOADING_FADE_S }}
    >
      <div className="max-w-3xl mx-auto w-full">
        <div className="py-2 text-xs text-muted-foreground">
          {shouldReduceMotion ? (
            <span>{message}</span>
          ) : (
            <Shimmer
              as="span"
              className="text-xs"
              tone="soft"
              delay={LOADING_SHIMMER_DELAY_S}
              duration={LOADING_SHIMMER_S}
              spread={LOADING_SHIMMER_SPREAD}
              repeatDelay={LOADING_SHIMMER_REPEAT_DELAY_S}
            >
              {message}
            </Shimmer>
          )}
        </div>
      </div>
    </motion.div>
  );
}
