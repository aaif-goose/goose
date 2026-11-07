// /statechart view for ToolDiscovery.tla.
const name: Record<number, string> = {
  1: "developer__shell",
  2: "github__get_me",
  3: "github__list_prs",
  4: "linear__get_issue",
  5: "linear__save_issue",
};

const extensions = (on: string[]) =>
  on.length === 0 ? "no extensions" : on.length === 2 ? "github and linear on" : `only ${on[0]} on`;

const loadedTools = (loaded: Record<string, number>) => {
  const names = Object.values(loaded ?? {}).map((id) => name[id]);
  return names.length === 0 ? "nothing loaded" : `loaded ${names.join(", ")}`;
};

const waiting: Record<string, string> = {
  idle: "",
  loads: ", load_tools calls waiting",
  calls: ", a tool call waiting",
};

export const title = "Progressive tool discovery in one deferring goose session";

export const state = (v: any) =>
  `${extensions(v.enabled)}, ${loadedTools(v.loaded)}${waiting[v.phase]}`;

export const step = (action: string, from: any, to: any) => {
  switch (action) {
    case "Infer":
      return `goose sends ${Object.keys(to.sent).length} tools and the model answers`;
    case "DiscoveryRun":
      return "goose answers the load_tools calls";
    case "CallAnswered":
      return "goose answers the tool call";
    case "Clear":
      return "you run /clear";
    case "LeaseLost":
      return "goose restarts before answering";
    case "ToggleExtension":
      return "an extension is turned on or off";
    default:
      return action;
  }
};

export const group = (v: any) => (v.everyFits ? "within" : "past");
export const colors = { within: "blue", past: "orange" };
