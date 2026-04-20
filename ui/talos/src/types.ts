export interface Model {
  id: string;
  name: string;
  desc: string;
  max: number;
  badge?: string;
}

export interface McpServer {
  id: string;
  name: string;
  desc: string;
  on: boolean;
}

export type ContextFolderKind = "default" | "project" | "picker";

export interface ContextFolder {
  id: string;
  name: string;
  kind: ContextFolderKind;
  desc?: string;
}

export type ChatGroup = "today" | "yesterday" | "this-week" | "older";

export interface Chat {
  id: string;
  title: string;
  group: ChatGroup;
  when: string;
  pinned?: boolean;
  project?: string;
}

export interface Project {
  id: string;
  name: string;
  color: string;
  chats: string[];
  notes: string[];
}

export type NoteKind = "wiki" | "note";

export type NoteBlock =
  | { type: "h1"; text: string }
  | { type: "h2"; text: string }
  | { type: "p"; text: string }
  | { type: "ul"; items: string[] }
  | { type: "meta" };

export interface Note {
  id: string;
  title: string;
  kind: NoteKind;
  updated: string;
  words: number;
  body: NoteBlock[];
}

export interface Workflow {
  id: string;
  name: string;
  lastRun: string;
  steps: number;
}

export interface Skill {
  id: string;
  name: string;
  desc: string;
  on: boolean;
}

export type MessageRole = "user" | "assistant";

export interface ToolUse {
  name: string;
  summary: string;
}

export type ToolStatus = "executing" | "completed" | "failed";

export interface ToolEvent extends ToolUse {
  id: string;
  status: ToolStatus;
}

export interface Message {
  id?: string;
  role: MessageRole;
  model?: string;
  paragraphs?: string[];
  bullets?: string[];
  followup?: string;
  /** Single inline tool pill (legacy; used by mock fixtures). */
  tool?: ToolUse;
  /** Tool events attached to this message as they stream in. */
  tools?: ToolEvent[];
  /** True while this assistant message is still being streamed. */
  streaming?: boolean;
}

export interface ChatTab {
  id: string;
  title: string;
  chatId: string | null;
  messages: Message[];
  composer: string;
  attachments: string[];
  /** Assigned lazily on first send. */
  gooseSessionId?: string;
}

export type SectionId = "chat" | "projects" | "memory" | "workflows" | "skills";

export interface SectionDef {
  id: SectionId;
  label: string;
  icon: string;
}

export interface Command {
  label: string;
  icon: string;
  kbd?: string;
  section?: string;
  run?: () => void;
}

export interface Toast {
  id: number;
  msg: string;
}
