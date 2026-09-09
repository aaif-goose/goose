import { stepCountIs, streamText } from "ai";
import type { Message, ThreadChannel } from "discord.js";
import { model } from "../../clients/ai";
import { logger } from "../logger";
import { chunkMarkdown } from "./chunk-markdown";
import { MAX_STEPS, buildSystemPrompt } from "./system-prompt";
import { ToolTracker } from "./tool-tracker";
import { aiTools } from "./tools";

export interface MessageHistoryItem {
  author: string;
  content: string;
  isBot: boolean;
}

export interface AnswerQuestionOptions {
  question: string;
  thread: ThreadChannel;
  userId: string;
  messageHistory?: MessageHistoryItem[];
  statusMessage?: Message;
  serverContext?: string;
}

export async function answerQuestion({
  question,
  thread,
  userId,
  messageHistory,
  statusMessage,
  serverContext,
}: AnswerQuestionOptions): Promise<void> {
  try {
    let prompt = question;
    if (messageHistory && messageHistory.length > 0) {
      const historyContext = messageHistory
        .slice(0, -1)
        .map((msg) => `${msg.author}: ${msg.content}`)
        .join("\n");

      if (historyContext) {
        prompt = `# Previous conversation\n${historyContext}\n\n# New message\n${messageHistory[messageHistory.length - 1].author}: ${question}`;
      }
    }

    const tracker = new ToolTracker();
    const systemPrompt = buildSystemPrompt(serverContext);

    const result = streamText({
      model,
      system: systemPrompt,
      prompt,
      tools: aiTools,
      stopWhen: stepCountIs(MAX_STEPS),
    });

    for await (const event of result.stream) {
      if (event.type === "tool-call") {
        if (statusMessage) {
          try {
            if (event.toolName === "search_docs") {
              await statusMessage.edit("Searching the docs...");
            } else if (event.toolName === "view_docs") {
              const input = event.input as { filePaths?: string | string[] };
              const filePaths = input.filePaths;
              const pathArray = Array.isArray(filePaths)
                ? filePaths
                : [filePaths];
              const pagesText = pathArray.length === 1 ? "page" : "pages";
              await statusMessage.edit(
                `Viewing ${pathArray.length} ${pagesText}...`,
              );
            } else if (event.toolName === "search_codebase") {
              await statusMessage.edit("Searching the codebase...");
            } else if (event.toolName === "view_codebase") {
              const input = event.input as { filePaths?: string | string[] };
              const filePaths = input.filePaths;
              const pathArray = Array.isArray(filePaths)
                ? filePaths
                : [filePaths];
              const filesText = pathArray.length === 1 ? "file" : "files";
              await statusMessage.edit(
                `Reading ${pathArray.length} source ${filesText}...`,
              );
            } else if (event.toolName === "list_codebase_files") {
              await statusMessage.edit("Exploring project structure...");
            } else if (event.toolName === "search_github") {
              await statusMessage.edit("Searching GitHub...");
            } else if (event.toolName === "get_github_issue_or_pr") {
              await statusMessage.edit("Reading GitHub issues and PRs...");
            }
          } catch (error) {
            logger.verbose("Failed to update status message:", error);
          }
        }
      } else if (event.type === "tool-result") {
        if (event.toolName === "search_docs") {
          const resultText = String(event.output);
          const fileMatches = resultText.match(/\*\*[^*]+\*\*/g) || [];
          tracker.recordSearchCall(fileMatches.map((_, i) => `result_${i}`));
        } else if (event.toolName === "view_docs") {
          const input = event.input as { filePaths?: string | string[] };
          const filePaths = input.filePaths;
          const pathArray = Array.isArray(filePaths)
            ? filePaths
            : filePaths
              ? [filePaths]
              : [];
          if (pathArray.length > 0) {
            tracker.recordViewCall(pathArray);
          }
        } else if (event.toolName === "search_codebase") {
          const resultText = String(event.output);
          const matchCount = (resultText.match(/\*\*[^*]+:\d+\*\*/g) || [])
            .length;
          tracker.recordCodeSearchCall(matchCount);
        } else if (event.toolName === "view_codebase") {
          const input = event.input as { filePaths?: string | string[] };
          const filePaths = input.filePaths;
          const pathArray = Array.isArray(filePaths)
            ? filePaths
            : filePaths
              ? [filePaths]
              : [];
          if (pathArray.length > 0) {
            tracker.recordCodeViewCall(pathArray);
          }
        } else if (event.toolName === "list_codebase_files") {
          tracker.recordListDir();
        } else if (event.toolName === "search_github") {
          tracker.recordGitHubSearch();
        } else if (event.toolName === "get_github_issue_or_pr") {
          tracker.recordGitHubRead();
        }
      }
    }

    if (statusMessage) {
      try {
        const summary = tracker.getSummary();
        await statusMessage.edit(summary || "Just a sec...");
      } catch (error) {
        logger.verbose("Failed to update final status message:", error);
      }
    }

    const fullText = await result.text;
    const chunks = chunkMarkdown(fullText);
    for (const chunk of chunks) {
      await thread.send(chunk);
    }

    const totalUsage = await result.usage;
    const { totalTokens } = totalUsage;
    logger.verbose(
      `Answered question for user ${userId}, tokens: ${totalTokens}`,
    );
  } catch (error) {
    logger.error("Failed to answer question:", error);
    await thread.send(
      "Sorry, I encountered an error while researching your question. Reply to this message or @mention me to try again.",
    );
    throw error;
  }
}
