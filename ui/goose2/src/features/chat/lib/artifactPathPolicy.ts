import type { Message } from "@/shared/types/messages";
import {
  confidenceRank,
  extractToolCallCandidates,
  filenameSignalBoost,
  isWriteOrientedTool,
  normalizeComparablePath,
  outputPathSignalBoost,
  sourceRank,
  type ArtifactPathCandidate,
  type BuildArtifactsIndexResult,
  type MessageArtifactsRanking,
  type ToolCallArtifactInput,
} from "./artifactPathPolicyCore";

export {
  evaluatePathScope,
  extractToolCallCandidates,
  inferHomeDirFromRoots,
  isWriteOrientedTool,
  normalizePath,
  resolveMarkdownLocalHref,
  resolvePathCandidate,
  type ArtifactCandidateConfidence,
  type ArtifactCandidateKind,
  type ArtifactCandidateSource,
  type ArtifactPathCandidate,
  type BuildArtifactsIndexResult,
  type MessageArtifactsRanking,
  type ToolCallArtifactInput,
} from "./artifactPathPolicyCore";

function compareCandidates(
  left: ArtifactPathCandidate,
  right: ArtifactPathCandidate,
  latestWriteToolCallIndex: number,
): number {
  const leftLatestWriteBoost =
    left.toolCallIndex === latestWriteToolCallIndex &&
    left.toolName &&
    isWriteOrientedTool(left.toolName)
      ? 1
      : 0;
  const rightLatestWriteBoost =
    right.toolCallIndex === latestWriteToolCallIndex &&
    right.toolName &&
    isWriteOrientedTool(right.toolName)
      ? 1
      : 0;
  if (leftLatestWriteBoost !== rightLatestWriteBoost) {
    return rightLatestWriteBoost - leftLatestWriteBoost;
  }

  const leftFilenameBoost = filenameSignalBoost(left.resolvedPath);
  const rightFilenameBoost = filenameSignalBoost(right.resolvedPath);
  if (leftFilenameBoost !== rightFilenameBoost) {
    return rightFilenameBoost - leftFilenameBoost;
  }

  const leftPathBoost = outputPathSignalBoost(left.resolvedPath);
  const rightPathBoost = outputPathSignalBoost(right.resolvedPath);
  if (leftPathBoost !== rightPathBoost) {
    return rightPathBoost - leftPathBoost;
  }

  if (left.appearanceIndex !== right.appearanceIndex) {
    return right.appearanceIndex - left.appearanceIndex;
  }

  const leftConfidence = confidenceRank(left.confidence);
  const rightConfidence = confidenceRank(right.confidence);
  if (leftConfidence !== rightConfidence) {
    return rightConfidence - leftConfidence;
  }

  return sourceRank(right.source) - sourceRank(left.source);
}

export function dedupeAndRankCandidates(
  candidates: ArtifactPathCandidate[],
): ArtifactPathCandidate[] {
  const hasWriteCandidates = candidates.some(
    (candidate) =>
      candidate.toolName && isWriteOrientedTool(candidate.toolName),
  );
  const candidatePool = hasWriteCandidates
    ? candidates.filter(
        (candidate) =>
          candidate.toolName && isWriteOrientedTool(candidate.toolName),
      )
    : candidates;

  const latestWriteToolCallIndex = candidates
    .filter(
      (candidate) =>
        candidate.toolName && isWriteOrientedTool(candidate.toolName),
    )
    .reduce((max, candidate) => Math.max(max, candidate.toolCallIndex), -1);

  const ranked = [...candidatePool].sort((left, right) =>
    compareCandidates(left, right, latestWriteToolCallIndex),
  );

  const deduped: ArtifactPathCandidate[] = [];
  const seenPaths = new Set<string>();
  for (const candidate of ranked) {
    const key = normalizeComparablePath(candidate.resolvedPath);
    if (!key || seenPaths.has(key)) continue;
    seenPaths.add(key);
    deduped.push(candidate);
  }
  return deduped;
}

export function rankMessageToolArtifacts(
  toolCalls: ToolCallArtifactInput[],
  allowedRoots: string[],
): MessageArtifactsRanking {
  let appearanceIndex = 0;
  const allCandidates: ArtifactPathCandidate[] = [];
  const argsByToolCallId = new Map<string, Record<string, unknown>>();

  for (const toolCall of toolCalls) {
    argsByToolCallId.set(toolCall.toolCallId, toolCall.args);
    const candidates = extractToolCallCandidates(
      toolCall,
      allowedRoots,
      appearanceIndex,
    );
    appearanceIndex += candidates.length;
    allCandidates.push(...candidates);
  }

  const ranked = dedupeAndRankCandidates(allCandidates);
  const firstAllowedIndex = ranked.findIndex((candidate) => candidate.allowed);
  const primaryIndex = firstAllowedIndex === -1 ? 0 : firstAllowedIndex;
  const primaryCandidate = ranked[primaryIndex] ?? null;
  const primaryToolCallId = primaryCandidate?.toolCallId ?? null;
  const secondaryCandidates = ranked.filter(
    (_candidate, index) => index !== primaryIndex,
  );

  const candidatesByToolCallId = new Map<string, ArtifactPathCandidate[]>();
  for (const toolCall of toolCalls) {
    candidatesByToolCallId.set(toolCall.toolCallId, []);
  }
  for (const candidate of ranked) {
    if (!candidate.toolCallId) continue;
    if (!candidatesByToolCallId.has(candidate.toolCallId)) {
      candidatesByToolCallId.set(candidate.toolCallId, []);
    }
    candidatesByToolCallId.get(candidate.toolCallId)?.push(candidate);
  }

  return {
    primaryToolCallId,
    primaryCandidate,
    secondaryCandidates,
    candidatesByToolCallId,
    argsByToolCallId,
  };
}

function toSafeRecord(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) return {};
  return value as Record<string, unknown>;
}

function findPreferredToolCallIdForText(
  orderedIds: string[],
  byId: Map<string, ToolCallArtifactInput>,
): string | null {
  for (let index = orderedIds.length - 1; index >= 0; index -= 1) {
    const toolCallId = orderedIds[index];
    const toolCall = byId.get(toolCallId);
    if (toolCall?.toolName && isWriteOrientedTool(toolCall.toolName)) {
      return toolCallId;
    }
  }

  return orderedIds[orderedIds.length - 1] ?? null;
}

function extractToolCallsFromMessage(
  message: Message,
): ToolCallArtifactInput[] {
  const byId = new Map<string, ToolCallArtifactInput>();
  const orderedIds: string[] = [];
  let toolCallIndex = 0;

  for (const block of message.content) {
    if (block.type === "toolRequest") {
      if (!byId.has(block.id)) {
        orderedIds.push(block.id);
        byId.set(block.id, {
          toolCallId: block.id,
          toolName: block.name,
          args: toSafeRecord(block.arguments),
          toolCallIndex,
        });
        toolCallIndex += 1;
      } else {
        const existing = byId.get(block.id);
        if (existing) {
          existing.toolName = block.name || existing.toolName;
          existing.args = toSafeRecord(block.arguments);
        }
      }
      continue;
    }

    if (block.type === "toolResponse") {
      if (!byId.has(block.id)) {
        orderedIds.push(block.id);
        byId.set(block.id, {
          toolCallId: block.id,
          toolName: block.name,
          args: {},
          result: block.result,
          toolCallIndex,
        });
        toolCallIndex += 1;
      } else {
        const existing = byId.get(block.id);
        if (existing) {
          existing.toolName = existing.toolName || block.name;
          existing.result = block.result;
        }
      }
    }

    if (block.type === "text") {
      const targetToolCallId = findPreferredToolCallIdForText(orderedIds, byId);
      if (!targetToolCallId) continue;
      const existing = byId.get(targetToolCallId);
      if (!existing) continue;
      existing.result = existing.result
        ? `${existing.result}\n${block.text}`
        : block.text;
    }
  }

  return orderedIds
    .map((toolCallId) => byId.get(toolCallId))
    .filter((value): value is ToolCallArtifactInput => Boolean(value));
}

export function buildArtifactsIndexForMessages(
  messages: Message[],
  allowedRoots: string[],
): BuildArtifactsIndexResult {
  const byMessageId = new Map<string, MessageArtifactsRanking>();
  const argsToToolCallId = new WeakMap<Record<string, unknown>, string>();

  for (const message of messages) {
    if (message.role !== "assistant") continue;
    if (message.metadata?.userVisible === false) continue;
    const toolCalls = extractToolCallsFromMessage(message);
    if (toolCalls.length === 0) continue;
    const ranking = rankMessageToolArtifacts(toolCalls, allowedRoots);
    byMessageId.set(message.id, ranking);

    for (const [toolCallId, args] of ranking.argsByToolCallId.entries()) {
      argsToToolCallId.set(args, toolCallId);
    }
  }

  return { byMessageId, argsToToolCallId };
}
