import {
  Output,
  ToolLoopAgent,
  tool,
  isStepCount,
  type ModelMessage,
  type UserContent,
} from "ai";
import type { Guild, ThreadChannel } from "discord.js";
import { z } from "zod";
import { model } from "../../clients/ai";
import { errorDetails, logger } from "../logger";
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
    stopWhen: isStepCount(MAX_STEPS),
    prepareStep: ({ stepNumber }) =>
      stepNumber >= MAX_STEPS - 1
        ? { activeTools: [], toolChoice: "none" }
        : undefined,
    timeout: { totalMs: 120_000, stepMs: 30_000 },
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
  const started = Date.now();
  let phase = "read-memory";
  let generation: Record<string, unknown> = {};
  try {
    const note = await memory.read(thread.id);
    const agent = createAnswerAgent(thread.guild);
    phase = "generate";
    const result = await agent.generate({
      onStepStart: ({ stepNumber }) => {
        generation.step = stepNumber + 1;
        logger.debug("AI step started", {
          threadId: thread.id,
          step: stepNumber + 1,
        });
      },
      onToolExecutionStart: ({ toolCall }) => {
        logger.debug("AI tool started", {
          threadId: thread.id,
          tool: toolCall.toolName,
          toolCallId: toolCall.toolCallId,
        });
      },
      onToolExecutionEnd: ({ toolCall, toolOutput, toolExecutionMs }) => {
        const context = {
          threadId: thread.id,
          tool: toolCall.toolName,
          toolCallId: toolCall.toolCallId,
          toolExecutionMs,
        };
        if (toolOutput.type === "tool-error") {
          logger.error("AI tool failed", {
            ...context,
            error: errorDetails(toolOutput.error),
          });
        } else {
          logger.debug("AI tool completed", context);
        }
      },
      onStepEnd: (step) => {
        generation = {
          step: step.stepNumber + 1,
          finishReason: step.finishReason,
          rawFinishReason: step.rawFinishReason,
          usage: step.usage,
          responseId: step.response.id,
          textLength: step.text.length,
          tools: step.toolCalls.map((call) => call.toolName),
          warnings: step.warnings,
        };
        logger.debug("AI step completed", {
          threadId: thread.id,
          ...generation,
          durationMs: step.performance.stepTimeMs,
        });
      },
      onEnd: ({ totalUsage, steps }) => {
        generation.totalUsage = totalUsage;
        generation.steps = steps.length;
      },
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
    phase = "parse-output";
    const output = result.output;
    const answer = output.answer.trim();
    if (!answer) throw new Error("Empty answer");
    phase = "send-answer";
    for (const content of chunkMarkdown(answer)) {
      await thread.send({ content, allowedMentions: { parse: [] } });
    }
    await memory.write(thread.id, output.memory).catch((error) =>
      logger.error("Failed to save thread memory", {
        threadId: thread.id,
        error: errorDetails(error),
      }),
    );
    logger.debug("Answered question", {
      threadId: thread.id,
      durationMs: Date.now() - started,
      ...generation,
    });
  } catch (error) {
    logger.error("Failed to answer question", {
      threadId: thread.id,
      model: model.modelId,
      phase,
      durationMs: Date.now() - started,
      ...generation,
      error: errorDetails(error),
    });
    await thread
      .send(
        "I couldn't finish looking into this. Reply or @mention me to retry.",
      )
      .catch((sendError) =>
        logger.error("Failed to send failure notice", {
          threadId: thread.id,
          error: errorDetails(sendError),
        }),
      );
  }
}
