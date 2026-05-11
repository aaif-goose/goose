import { SkillIcon } from "@/features/skills/ui/SkillIcon";
import { cn } from "@/shared/lib/cn";
import type { MessageChip } from "@/shared/types/messages";

const messageChipClasses: Record<MessageChip["type"], string> = {
  skill:
    "bg-yellow-100/25 text-yellow-700 dark:bg-yellow-100/10 dark:text-yellow-100",
  extension:
    "bg-blue-100/20 text-blue-700 dark:bg-blue-100/10 dark:text-blue-100",
  recipe:
    "bg-green-100/20 text-green-700 dark:bg-green-100/10 dark:text-green-100",
};

export function MessageMetadataChip({ chip }: { chip: MessageChip }) {
  const Icon = chip.type === "skill" ? SkillIcon : null;

  return (
    <span
      className={cn(
        "inline-flex h-6 max-w-64 items-center gap-1.5 rounded-full pl-[9px] pr-2 text-xs font-normal",
        messageChipClasses[chip.type],
      )}
    >
      {Icon ? <Icon className="size-3.5 shrink-0" /> : null}
      <span className="min-w-0 truncate">{chip.label}</span>
    </span>
  );
}
