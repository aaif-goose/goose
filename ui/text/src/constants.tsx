// UI Layout Constants
export const PASTE_THRESHOLD = 80;
export const PASTE_PREVIEW_LEN = 40;
export const INPUT_MAX_ROWS = 8;
export const SENT_PREVIEW_LEN = 60;

export const GOOSE_FRAMES = [
  [
    "    ,_",
    "   (o >",
    "   //\\",
    "   \\\\ \\",
    "    \\\\_/",
    "     |  |",
    "     ^ ^",
  ],
  [
    "     ,_",
    "    (o >",
    "    //\\",
    "    \\\\ \\",
    "     \\\\_/",
    "    /  |",
    "   ^   ^",
  ],
  [
    "    ,_",
    "   (o >",
    "   //\\",
    "   \\\\ \\",
    "    \\\\_/",
    "     |  |",
    "     ^  ^",
  ],
  [
    "   ,_",
    "  (o >",
    "  //\\",
    "  \\\\ \\",
    "   \\\\_/",
    "    |  \\",
    "    ^   ^",
  ],
];

export const GREETING_MESSAGES = [
  "What would you like to work on?",
  "Ready to build something amazing?",
  "What would you like to explore?",
  "What's on your mind?",
  "What shall we create today?",
  "What project needs attention?",
  "What would you like to tackle?",
  "What needs to be done?",
  "What's the plan for today?",
  "Ready to create something great?",
  "What can be built today?",
  "What's the next challenge?",
  "What progress can be made?",
  "What would you like to accomplish?",
  "What task awaits?",
  "What's the mission today?",
  "What can be achieved?",
  "What project is ready to begin?",
];

export const INITIAL_GREETING =
  GREETING_MESSAGES[Math.floor(Math.random() * GREETING_MESSAGES.length)]!;

export const PERMISSION_LABELS: Record<string, string> = {
  allow_once: "Allow once",
  allow_always: "Always allow",
  reject_once: "Reject once",
  reject_always: "Always reject",
};

export const PERMISSION_KEYS: Record<string, string> = {
  allow_once: "y",
  allow_always: "a",
  reject_once: "n",
  reject_always: "r",
};
