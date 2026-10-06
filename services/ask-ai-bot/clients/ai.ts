import { anthropic } from "@ai-sdk/anthropic";

const modelName = process.env.AI_MODEL || "claude-sonnet-5-5";

export const model = anthropic(modelName);
