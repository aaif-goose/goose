import type { Guild, TextChannel } from "discord.js";
import { ChannelType, PermissionFlagsBits } from "discord.js";

export async function buildServerContext(guild: Guild): Promise<string> {
  try {
    const channels = await guild.channels.fetch();

    const textChannels = Array.from(channels.values())
      .filter(
        (ch): ch is TextChannel =>
          ch?.type === ChannelType.GuildText &&
          !!ch
            .permissionsFor(guild.roles.everyone)
            ?.has(PermissionFlagsBits.ViewChannel),
      )
      .sort((a, b) => (a.position ?? 0) - (b.position ?? 0));

    if (textChannels.length === 0) {
      return "";
    }

    const channelList = textChannels
      .map(
        (ch) =>
          `- ID: ${ch.id}; Name: ${ch.name}; ${ch.topic ? `Topic: ${ch.topic}` : ""}`,
      )
      .join("\n");

    return `## Server Channels
If a user asks about the server's channels or where to find something, here's the current channel list:
${channelList}

When mentioning a channel, provide the link to the channel rather than using the plain text name. You can link to a channel by using the following format: \`<#channelId>\`.`;
  } catch (error) {
    console.error("Error building server context:", error);
    return "";
  }
}
