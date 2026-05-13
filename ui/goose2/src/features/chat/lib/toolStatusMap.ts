import type { ToolCallStatus } from "@/shared/types/messages";
import type { ToolPart } from "@/shared/ui/ai-elements/tool";

export const toolStatusMap: Record<ToolCallStatus, ToolPart["state"]> = {
  pending: "input-streaming",
  in_progress: "input-available",
  completed: "output-available",
  failed: "output-error",
  stopped: "output-denied",
};
