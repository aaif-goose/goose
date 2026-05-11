import type { ChatAttachmentDraft } from "@/shared/types/messages";
import type { ChatSkillDraft } from "../types";

export function attachmentSnapshotsMatch(
  current: ChatAttachmentDraft[],
  snapshot: ChatAttachmentDraft[],
) {
  return (
    current.length === snapshot.length &&
    current.every((attachment, index) => attachment.id === snapshot[index]?.id)
  );
}

export function skillDraftSnapshotsMatch(
  current: ChatSkillDraft[],
  snapshot: ChatSkillDraft[],
) {
  return (
    current.length === snapshot.length &&
    current.every((skill, index) => skill.id === snapshot[index]?.id)
  );
}
