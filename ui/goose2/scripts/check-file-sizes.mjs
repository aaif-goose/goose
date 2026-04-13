import { readFileSync, readdirSync } from "node:fs";
import { join, relative } from "node:path";

const DEFAULT_LIMIT = 500;

// Add narrowly scoped exceptions here with justification
const EXCEPTIONS = {
  "src/features/sidebar/ui/SidebarProjectsSection.tsx": {
    limit: 560,
    justification:
      "Drag-and-drop handlers plus activeProjectId highlight for draft-in-project sessions.",
  },
  "src/features/chat/ui/ChatView.tsx": {
    limit: 535,
    justification:
      "ACP prewarm guards, project-aware working dir selection, working context sync, and chat bootstrapping still live together here.",
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
    limit: 640,
    justification:
      "Shell still coordinates ACP session loading, project reassignment, and app-level chat routing.",
  },
  "src/features/chat/hooks/useAcpStream.ts": {
    limit: 580,
    justification:
      "ACP replay, streaming, session binding, model-state event handling, and replay timeout are still centralized here.",
  },
  "src/features/chat/hooks/__tests__/useAcpStream.test.ts": {
    limit: 570,
    justification:
      "Covers replay buffering, timeout error state, streaming edge cases, and provider identity persistence in one cohesive suite.",
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
  "src-tauri/src/services/acp/manager/dispatcher.rs": {
    limit: 540,
    justification:
      "ACP replay and live-stream event fan-out share one dispatcher with replay event counting for drain stabilisation.",
  },
  "src-tauri/src/services/acp/manager.rs": {
    limit: 630,
    justification:
      "ACP manager command dispatch loop — export/import/fork session ext_method dispatch adds boilerplate.",
  },
  "src-tauri/src/services/acp/manager/session_ops.rs": {
    limit: 620,
    justification:
      "Session prepare/load/list logic, working-dir updates, wait_for_replay_drain helper with iteration cap, and composite prepared-session reuse remain colocated while ACP session ownership stabilizes.",
  },
  "src-tauri/src/commands/system.rs": {
    limit: 620,
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
