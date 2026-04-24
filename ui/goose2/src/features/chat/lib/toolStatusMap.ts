import type { ToolCallStatus } from "@/shared/types/messages";
import type { ToolPart } from "@/shared/ui/ai-elements/tool";

export const toolStatusMap: Record<ToolCallStatus, ToolPart["state"]> = {
  pending: "input-streaming",
  executing: "input-available",
  completed: "output-available",
  error: "output-error",
  stopped: "output-denied",
};
