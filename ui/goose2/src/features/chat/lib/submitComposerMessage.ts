import type { SkillCommandMatch } from "@/features/skills/lib/skillChatPrompt";
import { isPromiseLike } from "@/shared/lib/isPromiseLike";
import type { ChatAttachmentDraft } from "@/shared/types/messages";
import type { ChatInputSendHandler, ChatSkillDraft } from "../types";
import { buildSkillSendPayload } from "./skillSendPayload";

interface SubmitComposerMessageOptions {
  text: string;
  attachments: ChatAttachmentDraft[];
  skills: ChatSkillDraft[];
  selectedPersonaId?: string | null;
  onSend: ChatInputSendHandler;
  resolveSkillSlashCommand: (
    message: string,
  ) => SkillCommandMatch<ChatSkillDraft> | null;
}

export async function submitComposerMessage({
  text,
  attachments,
  skills,
  selectedPersonaId,
  onSend,
  resolveSkillSlashCommand,
}: SubmitComposerMessageOptions) {
  const slashSkillCommand =
    skills.length === 0 ? resolveSkillSlashCommand(text) : null;
  const { messageText, sendOptions } = buildSkillSendPayload(
    text,
    skills,
    slashSkillCommand,
  );
  const submittedAttachments = attachments.length > 0 ? attachments : undefined;
  const sendResult = sendOptions
    ? onSend(
        messageText,
        selectedPersonaId ?? undefined,
        submittedAttachments,
        sendOptions,
      )
    : onSend(
        messageText.trim(),
        selectedPersonaId ?? undefined,
        submittedAttachments,
      );
  const accepted = isPromiseLike<boolean>(sendResult)
    ? await sendResult
    : sendResult;
  return accepted !== false;
}
