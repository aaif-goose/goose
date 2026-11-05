const operation = {
  name: "local-policy",

  prompt_parts() {
    return [["local-policy", "Explain the intended changes before editing files."]];
  },

  async run_command({ command }) {
    if (command.command !== "local-policy") {
      return { kind: "not_applicable" };
    }

    return {
      kind: "applied",
      status: "yielded",
      effects: [{
        type: "append_message",
        message: {
          id: null,
          role: "assistant",
          created: Math.floor(Date.now() / 1000),
          content: [{ type: "text", text: "Local policy: explain changes before editing files." }],
          metadata: { userVisible: true, agentVisible: true },
        },
      }],
    };
  },
};
