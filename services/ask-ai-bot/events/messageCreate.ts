import {
  ChannelType,
  Client,
  Events,
  Message,
  type OmitPartialGroupDMChannel,
} from "discord.js";
import { answerQuestion } from "../utils/ai";
import { buildServerContext } from "../utils/discord/server-context";
import { logger } from "../utils/logger";

const followUpInstructions = {
  embeds: [
    {
      title: "Want to ask a follow-up?",
      description:
        "Reply to one of my messages or @mention me in this thread so I know to answer.",
      color: 0x6a9f58,
    },
  ],
};

export default {
  event: Events.MessageCreate,
  handler: async (
    _client: Client,
    message: OmitPartialGroupDMChannel<Message<boolean>>,
  ) => {
    if (message.author.bot) return;

    const questionChannelId = process.env.QUESTION_CHANNEL_ID;
    const guild = message.guild;
    const serverContext = guild ? await buildServerContext(guild) : "";

    // Handle messages in threads
    if (message.channel.isThread()) {
      const parentChannelId =
        message.channel.parent?.id ?? message.channel.parentId;

      if (!questionChannelId) {
        logger.verbose(
          "QUESTION_CHANNEL_ID is not configured; ignoring thread message",
        );
        return;
      }

      if (!parentChannelId || parentChannelId !== questionChannelId) {
        logger.verbose(
          `Ignoring thread message from ${message.author.username} (thread not in question channel)`,
        );
        return;
      }

      try {
        // Check if the bot was mentioned or replied to
        const isMentioned = message.mentions.has(message.client.user?.id || "");

        let isReplyToBot = false;
        if (message.reference?.messageId) {
          isReplyToBot = await message.channel.messages
            .fetch(message.reference.messageId)
            .then((msg) => msg.author.bot)
            .catch(() => false);
        }

        if (!isMentioned && !isReplyToBot) {
          logger.verbose(
            `Ignoring thread message from ${message.author.username} (not mentioned or replied to)`,
          );
          return;
        }

        await message.channel.sendTyping();

        // Fetch last 10 messages from the thread for context
        const messages = await message.channel.messages.fetch({ limit: 10 });
        const sortedMessages = Array.from(messages.values())
          .reverse()
          .map((msg) => ({
            author:
              msg.author?.displayName || msg.author?.username || "Unknown",
            content: msg.content,
            isBot: msg.author.bot,
          }));

        await answerQuestion({
          question: message.content,
          thread: message.channel,
          userId: message.author.id,
          messageHistory: sortedMessages,
          serverContext,
        });

        logger.verbose(
          `Answered follow-up question for ${message.author.username} in thread`,
        );
      } catch (error) {
        logger.error(`Error handling thread message: ${error}`);
      }
      return;
    }

    // Handle initial questions in the question channel
    if (questionChannelId && message.channelId === questionChannelId) {
      if (message.channel.type === ChannelType.GuildText) {
        try {
          let threadName = message.content.trim();
          if (threadName.length > 100) {
            threadName = threadName.substring(0, 97) + "...";
          }

          const thread = await message.startThread({
            name: threadName,
            autoArchiveDuration: 60,
          });

          // Send status message that will be updated as tools are called
          const statusMessage = await thread.send("Just a sec...");

          await answerQuestion({
            question: message.content,
            thread,
            userId: message.author.id,
            statusMessage,
            serverContext,
          });

          await thread.send(followUpInstructions);

          logger.verbose(`Answered question for ${message.author.username}`);
        } catch (error) {
          logger.error(`Error handling question: ${error}`);
        }
      }
    }
  },
};
