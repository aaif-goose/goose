import { readFileSync, readdirSync } from "node:fs";
import { join, relative } from "node:path";

const DEFAULT_LIMIT = 500;

// Add narrowly scoped exceptions here with justification
const EXCEPTIONS = {
  "src/features/sidebar/ui/SidebarProjectsSection.tsx": {
    limit: 570,
    justification:
      "Drag-and-drop handlers for session-to-project moves and project reorder, plus activeProjectId highlight.",
  },
  "src/features/chat/ui/ChatView.tsx": {
    limit: 570,
    justification:
      "ACP prewarm guards, project-aware working dir selection, working context sync, chat bootstrapping, context-ring compaction wiring, and gated [perf:chatview] logging via perfLog (dev-only by default).",
  },
  "src/features/chat/hooks/useChat.ts": {
    limit: 510,
    justification:
      "Session preparation, provider/model handoff, persona-aware sends, cancellation, and compaction replay still live in one chat lifecycle hook.",
  },
  "src/shared/api/acpNotificationHandler.ts": {
    limit: 550,
    justification:
      "ACP replay/live update handling, pending session buffering, model/config propagation, and streaming perf tracking still share one notification entrypoint.",
  },
  "src/features/chat/ui/__tests__/ContextPanel.test.tsx": {
    limit: 550,
    justification:
      "Workspace widget integration tests cover branch switching, worktree creation, dirty-state dialogs, and picker interactions.",
  },
  "src/features/sidebar/ui/Sidebar.tsx": {
    limit: 580,
    justification:
      "Search-as-you-type filtering and draft-aware sidebar highlight logic.",
  },
  "src/app/AppShell.tsx": {
    limit: 780,
    justification:
      "Shell still coordinates ACP session loading, replay-buffer cleanup on load failure, project reassignment, home-session restoration, app-level chat routing, restored project-draft reuse, and app-level compaction settings deep links. Includes gated [perf:load]/[perf:newtab] logging via perfLog (dev-only by default).",
  },
  "src/features/chat/hooks/useChatSessionController.ts": {
    limit: 840,
    justification:
      "Controller now centralizes home-to-chat pending state transfer, workspace/project preparation, provider/model/persona handoff, Goose cross-provider model selection sequencing with rollback, context-usage readiness resets, queued-target compaction gating, and auto-compaction-aware send orchestration pending a later decomposition pass.",
  },
  "src/features/chat/hooks/__tests__/useChatSessionController.test.ts": {
    limit: 520,
    justification:
      "Controller regression coverage now spans model/provider rollback, stale usage resets, compact-before-send, and queued-persona auto-compaction support checks in one hook suite.",
  },
  "src/features/chat/stores/chatStore.ts": {
    limit: 520,
    justification:
      "Chat runtime state, queued-message persistence, replay loading flags, and usage snapshot tracking still live together in one Zustand store.",
  },
  "src/features/chat/ui/AgentModelPicker.tsx": {
    limit: 570,
    justification:
      "Agent-first picker currently keeps the full trigger, recommended-model view, searchable full-model view, and ACP/goose-specific labeling logic in one component pending later extraction.",
  },
  "src/features/chat/stores/__tests__/chatSessionStore.test.ts": {
    limit: 540,
    justification:
      "ACP session overlay regressions currently need one broad integration-style store suite.",
  },
  "src/features/chat/stores/chatSessionStore.ts": {
    limit: 640,
    justification:
      "ACP-backed session overlay persistence, draft migration, and sidebar-facing session merge logic live together for now.",
  },
  "src/features/chat/ui/ChatInput.tsx": {
    limit: 510,
    justification:
      "Voice dictation send/stop guards, attachment handling, and mention/picker coordination still share one chat composer component.",
  },
  "src/features/chat/ui/MessageBubble.tsx": {
    limit: 520,
    justification:
      "Bubble rendering still owns assistant identity, grouped tool output, attachments, and the inline actions tray pending a later extraction pass.",
  },
  "src/features/chat/ui/__tests__/ChatInput.test.tsx": {
    limit: 570,
    justification:
      "Composer regression coverage spans personas, queueing, attachments, voice-input edge cases, and the compaction popover/settings ingress in one interaction-heavy suite.",
  },
  "src-tauri/src/commands/projects.rs": {
    limit: 520,
    justification:
      "Project CRUD plus reorder_projects command for sidebar drag-and-drop ordering.",
  },
  "src-tauri/src/commands/system.rs": {
    limit: 640,
    justification:
      "Desktop system commands still centralize file mentions, attachment inspection, platform-aware path dedupe, guarded image loading, and export helpers in one Tauri command surface.",
  },
};

// Directories excluded from size checks (imported library code)
const EXCLUDED_DIRS = [
  "src/shared/ui",
  "src/components/ai-elements",
  "src/hooks",
];

const DIRS_TO_CHECK = [
  { dir: "src/app", glob: /\.[jt]sx?$/ },
  { dir: "src/features", glob: /\.[jt]sx?$/ },
  { dir: "src/shared", glob: /\.[jt]sx?$/ },
  { dir: "src/components", glob: /\.[jt]sx?$/ },
  { dir: "src/hooks", glob: /\.[jt]sx?$/ },
  { dir: "src-tauri/src", glob: /\.rs$/ },
];

function countLines(filePath) {
  const content = readFileSync(filePath, "utf8");
  return content.split("\n").length;
}

function isExcluded(filePath) {
  const rel = relative(".", filePath);
  return EXCLUDED_DIRS.some((dir) => rel.startsWith(dir));
}

function walkDir(dir, pattern) {
  const results = [];
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return results;
  }
  for (const entry of entries) {
    const fullPath = join(dir, entry.name);
    if (entry.isDirectory()) {
      results.push(...walkDir(fullPath, pattern));
    } else if (pattern.test(entry.name)) {
      results.push(fullPath);
    }
  }
  return results;
}

const violations = [];

for (const { dir, glob } of DIRS_TO_CHECK) {
  const files = walkDir(dir, glob);
  for (const file of files) {
    if (isExcluded(file)) continue;
    const rel = relative(".", file);
    const limit = EXCEPTIONS[rel]?.limit ?? DEFAULT_LIMIT;
    const lines = countLines(file);
    if (lines > limit) {
      violations.push({ file: rel, lines, limit });
    }
  }
}

if (violations.length > 0) {
  console.error("Desktop file size check failed:");
  for (const v of violations) {
    console.error(`  - ${v.file}: ${v.lines} lines (limit ${v.limit})`);
  }
  console.error(
    "\nSplit the file or add a narrowly scoped exception in `scripts/check-file-sizes.mjs`.",
  );
  process.exit(1);
} else {
  console.log("File size check passed.");
}
