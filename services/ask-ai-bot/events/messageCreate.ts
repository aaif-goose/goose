import type { ModelMessage } from "ai";
import {
  ChannelType,
  Client,
  Events,
  Message,
  type OmitPartialGroupDMChannel,
  type ThreadChannel,
} from "discord.js";
import { answerQuestion } from "../utils/ai";
import { inThreadOrder } from "../utils/discord/thread-memory";
import {
  messageContent,
  redactSecrets,
} from "../utils/discord/message-content";
import { logger } from "../utils/logger";

async function conversationHistory(
  thread: ThreadChannel,
  before: string,
  botId: string,
): Promise<ModelMessage[]> {
  const [starter, recent] = await Promise.all([
    thread.fetchStarterMessage().catch(() => null),
    thread.messages.fetch({ before, limit: 12 }),
  ]);
  const messages = [...recent.values()].filter(
    (item) => item.id !== starter?.id,
  );
  messages.sort(
    (a, b) =>
      a.createdTimestamp - b.createdTimestamp ||
      (BigInt(a.id) < BigInt(b.id) ? -1 : 1),
  );
  if (starter && BigInt(starter.id) < BigInt(before)) messages.unshift(starter);
  return messages
    .filter(
      (item) => item.content && (!item.author.bot || item.author.id === botId),
    )
    .map((item) => ({
      role: item.author.id === botId ? "assistant" : "user",
      content: redactSecrets(
        item.author.id === botId
          ? item.content
          : `${item.author.displayName}: ${item.content}`,
      ),
    }));
}

export default {
  event: Events.MessageCreate,
  handler: async (
    _client: Client,
    message: OmitPartialGroupDMChannel<Message<boolean>>,
  ) => {
    const questionChannelId = process.env.QUESTION_CHANNEL_ID;
    if (message.author.bot || !questionChannelId) return;
    const channel = message.channel;
    if (channel.isThread()) {
      if (channel.parentId !== questionChannelId) return;
      await inThreadOrder(channel.id, async () => {
        const botId = message.client.user!.id;
        const isMentioned = message.mentions.users.has(botId);
        const reply = message.reference?.messageId
          ? await channel.messages
              .fetch(message.reference.messageId)
              .catch(() => null)
          : null;
        if (!isMentioned && reply?.author.id !== botId) return;
        await respond(
          channel,
          message,
          await conversationHistory(channel, message.id, botId),
        );
      }).catch((error) => logger.error("Error handling follow-up:", error));
    } else if (
      message.channelId === questionChannelId &&
      channel.type === ChannelType.GuildText
    ) {
      try {
        const thread = await message.startThread({
          name: (message.content.trim() || "goose question").slice(0, 100),
          autoArchiveDuration: 60,
        });
        await inThreadOrder(thread.id, () => respond(thread, message));
      } catch (error) {
        logger.error("Error handling question:", error);
      }
    }
  },
};

async function respond(
  thread: ThreadChannel,
  message: Message,
  messages: ModelMessage[] = [],
): Promise<void> {
  await thread.sendTyping();
  const statusMessage = await thread.send("Looking into it…");
  try {
    await answerQuestion({
      question: await messageContent(message),
      thread,
      messages,
    });
  } finally {
    await statusMessage.delete().catch(() => {});
  }
}
