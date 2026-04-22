// Sample data for Tandem AI Workspace

const USER = { firstName: 'Maxim', fullName: 'Maxim Cherepanov', initials: 'MC' };

const MODELS = [
  { id: 'opus-4.7', name: 'Claude Opus 4.7', desc: 'Most capable, best for hard problems', max: 200000, badge: 'Flagship' },
  { id: 'sonnet-4.6', name: 'Claude Sonnet 4.6', desc: 'Balanced speed and intelligence', max: 1000000, badge: 'Default' },
  { id: 'haiku-4.5', name: 'Claude Haiku 4.5', desc: 'Fast and efficient', max: 200000 },
  { id: 'opus-4.7-extended', name: 'Claude Opus 4.7 (extended)', desc: 'Deep thinking mode', max: 200000 },
];

const MCP_SERVERS = [
  { id: 'filesystem', name: 'Filesystem', desc: 'Local file access', on: true },
  { id: 'github', name: 'GitHub', desc: 'Repos, issues, PRs', on: true },
  { id: 'linear', name: 'Linear', desc: 'Issue tracking', on: true },
  { id: 'postgres', name: 'Postgres', desc: 'Database queries', on: false },
  { id: 'slack', name: 'Slack', desc: 'Team messaging', on: false },
  { id: 'playwright', name: 'Playwright', desc: 'Browser automation', on: false },
  { id: 'memory', name: 'Memory graph', desc: 'Knowledge graph store', on: true },
];

const CONTEXT_FOLDERS = [
  { id: 'memory', name: 'Memory', kind: 'default', desc: 'Shared across everything' },
  { id: 'proj-tandem', name: 'Tandem UI', kind: 'project' },
  { id: 'proj-acp', name: 'ACP protocol', kind: 'project' },
  { id: 'proj-onboarding', name: 'Customer onboarding', kind: 'project' },
  { id: 'proj-research', name: 'Q2 research', kind: 'project' },
  { id: 'last-folder', name: 'Last opened folder…', kind: 'picker' },
];

// Chat sessions
const CHATS = [
  { id: 'c1', title: 'WebSocket singleton race condition', group: 'today', when: 'just now', pinned: true, project: 'proj-acp' },
  { id: 'c2', title: 'PRD review — Tandem workspace shell', group: 'today', when: '2h ago', pinned: true, project: 'proj-tandem' },
  { id: 'c3', title: 'Onboarding email sequence v3', group: 'today', when: '3h ago', pinned: true, project: 'proj-onboarding' },
  { id: 'c4', title: 'Refactor ACP notification handler', group: 'today', when: '4h ago', project: 'proj-acp' },
  { id: 'c5', title: 'Bases vs Notion databases', group: 'today', when: '5h ago' },
  { id: 'c6', title: 'Weekly status draft', group: 'today', when: '6h ago' },
  { id: 'c7', title: 'Plugin contract sketch', group: 'yesterday', when: 'Yesterday', project: 'proj-tandem' },
  { id: 'c8', title: 'Fix Groovy parser edge case', group: 'yesterday', when: 'Yesterday' },
  { id: 'c9', title: 'Calendar plugin API design', group: 'yesterday', when: 'Yesterday' },
  { id: 'c10', title: 'Dark theme audit', group: 'yesterday', when: 'Yesterday', project: 'proj-tandem' },
  { id: 'c11', title: 'MongoDB query optimization', group: 'yesterday', when: 'Yesterday' },
  { id: 'c12', title: 'Skill bundle format', group: 'this-week', when: 'Wed' },
  { id: 'c13', title: 'Interview rubric draft', group: 'this-week', when: 'Tue' },
  { id: 'c14', title: 'React Native WebSocket port', group: 'this-week', when: 'Tue', project: 'proj-acp' },
  { id: 'c15', title: 'Workflow engine v0', group: 'this-week', when: 'Mon' },
  { id: 'c16', title: 'Q2 research notes compile', group: 'this-week', when: 'Mon', project: 'proj-research' },
  { id: 'c17', title: 'Customer churn analysis', group: 'this-week', when: 'Mon', project: 'proj-research' },
  { id: 'c18', title: 'Token counter UX exploration', group: 'older', when: 'Apr 12', project: 'proj-tandem' },
  { id: 'c19', title: 'Memory auto-creation heuristics', group: 'older', when: 'Apr 10' },
  { id: 'c20', title: 'Sync conflict strategy', group: 'older', when: 'Apr 8' },
  { id: 'c21', title: 'Keyboard shortcut review', group: 'older', when: 'Apr 5' },
  { id: 'c22', title: 'Ribbon icon set decision', group: 'older', when: 'Apr 2', project: 'proj-tandem' },
];

// Projects
const PROJECTS = [
  { id: 'proj-tandem', name: 'Tandem UI', color: 'violet', chats: ['c2', 'c7', 'c10', 'c18', 'c22'], notes: ['n1', 'n5'] },
  { id: 'proj-acp', name: 'ACP protocol', color: 'blue', chats: ['c1', 'c4', 'c14'], notes: ['n2', 'n3'] },
  { id: 'proj-onboarding', name: 'Customer onboarding', color: 'amber', chats: ['c3'], notes: ['n7'] },
  { id: 'proj-research', name: 'Q2 research', color: 'green', chats: ['c16', 'c17'], notes: ['n8', 'n9'] },
];

// Notes (memory)
const NOTES = {
  n1: {
    id: 'n1',
    title: 'Tandem plugin contract',
    kind: 'wiki',
    updated: '2h ago',
    words: 412,
    body: [
      { type: 'h1', text: 'Tandem plugin contract' },
      { type: 'meta' },
      { type: 'p', text: 'Each plugin is a self-contained module that registers with the shell through a declarative manifest plus a runtime handle. The shell owns layout; the plugin owns its agent and its surfaces.' },
      { type: 'h2', text: 'Manifest fields' },
      { type: 'ul', items: [
        'id — stable, machine-readable identifier',
        'name — human label shown in tooltips',
        'ribbon_icon — Lucide name or svg path',
        'navigation_provider — left-panel content factory',
        'agent — registered under /agent-name',
        'context_provider — right-panel surface',
        'skills — optional contributed skills'
      ]},
      { type: 'h2', text: 'Lifecycle' },
      { type: 'p', text: 'Plugins init lazily. The shell calls the navigation provider only when the ribbon icon becomes active, so cold-start stays under 2 seconds even with many plugins installed.' },
    ]
  },
  n2: {
    id: 'n2',
    title: 'ACP WebSocket singleton pattern',
    kind: 'note',
    updated: 'just now',
    words: 289,
    body: [
      { type: 'h1', text: 'ACP WebSocket singleton pattern' },
      { type: 'meta' },
      { type: 'p', text: 'The app should hold exactly one connection to the ACP server. Cache the Promise, not the resolved client — three concurrent getClient() calls must share the same in-flight connection attempt.' },
      { type: 'h2', text: 'Why null the Promise on error' },
      { type: 'p', text: 'If we cached the rejected Promise forever, every subsequent getClient() call would re-throw the old error without trying. Nulling it means "the previous attempt failed; next caller should try fresh."' },
      { type: 'h2', text: 'Monitoring with .closed' },
      { type: 'p', text: 'Active pings add overhead and can trigger server-side rate limits. The .closed Promise resolves exactly when the socket dies, which is all we actually care about.' },
    ]
  },
  n3: {
    id: 'n3',
    title: 'Decision: TCP readiness check',
    kind: 'note',
    updated: 'Yesterday',
    words: 134,
    body: [
      { type: 'h1', text: 'Decision: TCP readiness check' },
      { type: 'meta' },
      { type: 'p', text: 'We poll the port with a 100ms delay up to 20 attempts before giving up. Bun\'s socket API resolves before the accept loop is actually listening, so a naive WebSocket connect right after spawn fails reliably.' },
    ]
  },
  n4: {
    id: 'n4',
    title: 'Daily — 2026-04-20',
    kind: 'note',
    updated: '5m ago',
    words: 88,
    body: [
      { type: 'h1', text: 'Daily — 2026-04-20' },
      { type: 'meta' },
      { type: 'p', text: 'Shipped the PRD draft for Tandem. Found a nasty WebSocket race condition — three getClient() calls in parallel opened three sockets. Fix in progress.' },
      { type: 'ul', items: [
        'PRD v0.1 sent for review',
        'ACP singleton patch drafted',
        'Reviewed onboarding email sequence'
      ]},
    ]
  },
  n5: {
    id: 'n5',
    title: 'Ribbon icon rationale',
    kind: 'note',
    updated: '3d ago',
    words: 176,
    body: [
      { type: 'h1', text: 'Ribbon icon rationale' },
      { type: 'meta' },
      { type: 'p', text: 'Emoji read as friendlier than Lucide here — the ribbon is the first thing users see, and the slightly playful tone mitigates the "yet another VS Code clone" reaction. Tooltips carry the precise plugin name.' },
      { type: 'p', text: 'If this becomes a problem in enterprise settings, we can ship a preference toggle later.' },
    ]
  },
  n6: {
    id: 'n6',
    title: 'Keyboard shortcut conventions',
    kind: 'wiki',
    updated: 'Apr 15',
    words: 203,
    body: [
      { type: 'h1', text: 'Keyboard shortcut conventions' },
      { type: 'meta' },
      { type: 'p', text: 'Tandem follows the Obsidian-ish convention: ⌘+letter for primary actions, ⌘⇧+letter for paired or inverse actions. ⌘K is reserved for the command palette.' },
    ]
  },
  n7: {
    id: 'n7',
    title: 'Onboarding checklist',
    kind: 'note',
    updated: '3h ago',
    words: 94,
    body: [
      { type: 'h1', text: 'Onboarding checklist' },
      { type: 'meta' },
      { type: 'p', text: 'Send welcome at day 0, tool-setup guide at day 2, check-in at day 7, feedback survey at day 14.' },
    ]
  },
  n8: {
    id: 'n8',
    title: 'Q2 research — top themes',
    kind: 'note',
    updated: 'Mon',
    words: 412,
    body: [
      { type: 'h1', text: 'Q2 research — top themes' },
      { type: 'meta' },
      { type: 'p', text: 'Across 14 customer interviews, three themes dominated: context-switching fatigue, fear of AI hallucinations on their own data, and frustration with pricing opacity.' },
    ]
  },
  n9: {
    id: 'n9',
    title: 'Churn analysis — power users',
    kind: 'note',
    updated: 'Mon',
    words: 221,
    body: [
      { type: 'h1', text: 'Churn analysis — power users' },
      { type: 'meta' },
      { type: 'p', text: 'Power users churn slower than median but when they leave, they cite missing integrations as the top reason.' },
    ]
  },
  n10: {
    id: 'n10',
    title: 'Weekly status — template',
    kind: 'wiki',
    updated: 'Apr 8',
    words: 112,
    body: [
      { type: 'h1', text: 'Weekly status — template' },
      { type: 'meta' },
      { type: 'p', text: 'Shipped / In progress / Blocked / Up next — one bullet each, plus a one-line morale check. Keep it under 200 words.' },
    ]
  },
};

// Session-scoped notes (for the active chat c1)
const SESSION_NOTES_BY_CHAT = {
  c1: ['n2', 'n3'],
  c2: ['n1', 'n5'],
  c3: ['n7'],
  c4: ['n2'],
  c16: ['n8', 'n9'],
};

const WORKFLOWS = [
  { id: 'w1', name: 'Weekly status digest', lastRun: 'Mon 9:02 AM', steps: 5 },
  { id: 'w2', name: 'New customer onboarding check', lastRun: 'Apr 14', steps: 8 },
  { id: 'w3', name: 'Inbox triage', lastRun: '2h ago', steps: 3 },
  { id: 'w4', name: 'Meeting prep brief', lastRun: 'Yesterday', steps: 4 },
  { id: 'w5', name: 'Code review assistant', lastRun: '3d ago', steps: 6 },
  { id: 'w6', name: 'Daily journal prompt', lastRun: '6 AM', steps: 2 },
];

const SKILLS = [
  { id: 's1', name: 'Read PDFs', desc: 'Extract text and tables from PDF files', on: true },
  { id: 's2', name: 'Generate docx', desc: 'Build Word documents from markdown', on: true },
  { id: 's3', name: 'Query MongoDB', desc: 'Run find() and aggregate() queries', on: true },
  { id: 's4', name: 'Render Mermaid diagrams', desc: 'Turn mermaid into SVG inline', on: true },
  { id: 's5', name: 'Web search', desc: 'Fetch and summarize pages', on: true },
  { id: 's6', name: 'Excalidraw import', desc: 'Parse .excalidraw files as scene data', on: false },
  { id: 's7', name: 'Generate PPTX', desc: 'Assemble PowerPoint decks', on: false },
  { id: 's8', name: 'OCR images', desc: 'Extract text from screenshots', on: true },
  { id: 's9', name: 'Send Slack message', desc: 'Post to channels or DMs', on: false },
];

// Canned AI reply used when sending a first message
const CANNED_REPLIES = {
  default: {
    text: [
      'Good question. Let me think through this with you.',
      'A few things stand out here:',
    ],
    bullets: [
      'The scope folder defaults to *Memory*, which means context leaks across sessions unless a project is set.',
      'The token counter should live in the composer footer — that keeps the Claude Code pattern intact.',
      'Auto-compact at 80% is a fine default, but we should expose it per-model since context windows vary a lot.',
    ],
    followup: 'Want me to draft the plugin manifest schema next, or dig into the auto-compact threshold logic?',
  },
};
