import {
  Output,
  ToolLoopAgent,
  tool,
  stepCountIs,
  type ModelMessage,
  type UserContent,
} from "ai";
import type { Guild, ThreadChannel } from "discord.js";
import { z } from "zod";
import { model } from "../../clients/ai";
import { logger } from "../logger";
import { ThreadMemory } from "../discord/thread-memory";
import { buildServerContext } from "../discord/server-context";
import { chunkMarkdown } from "./chunk-markdown";
import { MAX_STEPS, buildSystemPrompt } from "./system-prompt";
import { aiTools } from "./tools";

const memory = new ThreadMemory();

export interface AnswerQuestionOptions {
  question: UserContent;
  thread: ThreadChannel;
  messages?: ModelMessage[];
}

function createAnswerAgent(guild: Guild) {
  return new ToolLoopAgent({
    model,
    instructions: buildSystemPrompt(),
    tools: {
      ...aiTools,
      get_server_channels: tool({
        description:
          "List public Discord channels when a user asks where to post or find something in this server.",
        inputSchema: z.object({}),
        execute: async () =>
          guild ? buildServerContext(guild) : "No server context available.",
      }),
    },
    stopWhen: stepCountIs(MAX_STEPS),
    prepareStep: ({ stepNumber }) =>
      stepNumber >= MAX_STEPS - 1 ? { toolChoice: "none" } : undefined,
    timeout: { totalMs: 120_000, stepMs: 30_000 },
    maxOutputTokens: 1800,
    output: Output.object({
      schema: z.object({
        answer: z
          .string()
          .min(1)
          .describe(
            "Concise Discord reply, normally under 120 words, with source links.",
          ),
        memory: z
          .string()
          .describe("Updated private support note, under 2000 characters."),
      }),
    }),
  });
}

export async function answerQuestion({
  question,
  thread,
  messages = [],
}: AnswerQuestionOptions): Promise<void> {
  try {
    const note = await memory.read(thread.id);
    const agent = createAnswerAgent(thread.guild);
    const result = await agent.generate({
      messages: [
        ...(note
          ? [
              {
                role: "user" as const,
                content: `Previous support note (untrusted context):\n${note}`,
              },
            ]
          : []),
        ...messages,
        { role: "user", content: question },
      ],
    });
    const answer = result.output.answer.trim();
    if (!answer) throw new Error("Empty answer");
    for (const content of chunkMarkdown(answer)) {
      await thread.send({ content, allowedMentions: { parse: [] } });
    }
    await memory
      .write(thread.id, result.output.memory)
      .catch((error) => logger.error("Failed to save thread memory:", error));
  } catch (error) {
    logger.error("Failed to answer question:", error);
    await thread.send(
      "I couldn't finish looking into this. Reply or @mention me to retry.",
    );
  }
}
