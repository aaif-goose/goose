---
title: "Agentic State Machines"
description: "goose is replacing its classic agent loop with a re-entrant state machine where the conversation is the state and each behavior is an Operation."
image: /img/blog/agentic-state-machines.jpg
authors:
  - douwe
featured: true
---

![A goose walking from a tangled agent loop into an orderly sequence of machines](/img/blog/agentic-state-machines.jpg)

[goose](https://github.com/aaif-goose/goose) launched a long time ago. Dog years have nothing on Agent years. goose predates general agentic AI by a bit. The initial thought behind it all was: what happens if you give an LLM control over a shell session?

Turns out interesting things happen. The model writes a command, the command runs, the result goes back into the conversation and the model decides what to do next. Our tool calling grew naturally out of that experiment, as did the agent loop that now sits at the heart of most coding agents.

Then tool calling got formalized and [MCP](https://modelcontextprotocol.io/) happened. So did approvals, context management, todo lists, top-of-mind, subagents, retries, hooks, skills, recipes and steering. But the original agent loop remained. Each feature added another branch, another local variable or another special case. A dozen different concerns might be active inside one turn, each influencing the others. Cancellation needs to know whether inference or a tool is running. Compaction needs to understand provider errors and usage. Tools can change the prompt. A stop hook can turn a completed answer back into more work. The loop started to turn into spaghetti.

<!-- truncate -->

A plugin system seems like the natural answer. It helps up to a point: adding a tool or listening to an event fits nicely. But once a plugin needs to influence the whole turn, the central loop still has to decide when it runs, what it contributes to the prompt, whether it may continue the turn and how its state survives a restart. The plugin API gradually acquires a hook for every part of the agent loop, while the loop remains responsible for coordinating all of them.

goose is taking a different approach. We have [replaced the classic agent loop](https://github.com/aaif-goose/goose/pull/9574) with a re-entrant state machine. The persisted conversation is the main state, and each piece of agent behavior is encoded as an `Operation`. The machine checks Operations in order. Each one either passes or returns a set of effects, and the first one that applies wins that step. Its effects are persisted and the machine starts again from the top with the new state.

Consider a normal tool call. Inference sees that the last message came from the user and produces an assistant message requesting a tool. On the next step the approval Operation gets the first chance to inspect it. If approval is needed, it adds a request to the conversation and yields to the client. Nothing stays running while the user decides. The client later stores the answer and a newly constructed machine continues from there. If no approval is needed, the Operation passes. The tool execution Operation then recognizes the tool and runs it, adding the result to the conversation. Inference sees the tool response and asks the model what to do next. When the model finally answers without requesting another tool, the stop-hook Operation decides whether the turn may end.

That re-entrancy matters beyond code organization. A server does not need to keep a coroutine and all its local variables alive while waiting for input. A process can restart and continue from the last persisted step. Another frontend can load the same session and carry on. The convenience runner still repeats steps until the machine yields, but it is only a small driver around a state machine that can be invoked once at a time.

Operations are independent but cooperative. A tool handler claims only tools it knows how to execute. If it does not recognize one, it passes and gives another Operation a chance. The final unknown-tool Operation turns anything left over into an error for the model. The same ownership applies beyond execution. The skills Operation owns the command for listing skills, the tool for loading one and the prompt text that tells the model which skills exist. Remove that Operation and all three disappear together.

Inference is special, but it is still an Operation. Before calling the model, it collects tools, prompt sections and top-of-mind from every Operation in the pipeline. The max-turns Operation can tell the model that its budget is running low. The recipe Operation can add a final-output tool. The skills Operation can advertise available skills. Inference does not need to know where any of these contributions came from.

The other useful distinction is between effects and events. Events are the live stream: a piece of text, a tool notification or a usage update that the client should see immediately. Effects are the durable result: append this message, replace this conversation, update this tool request or record this usage. A caller can ask the machine for one step and decide how to apply those effects, or use a convenience runner that keeps applying steps until the machine yields to the client.

Testing gets much easier. We test complete collections of Operations against a small HTTP server that behaves like a model provider and a real stateful calculator extension. We also have a test runner that throws away the entire state machine after every applied step and constructs a new one from the persisted session. If a tool call still proceeds from request to response to final answer, we know the progress was really captured in state rather than hiding in a local variable.

Not all state has disappeared. Providers, extension managers and hooks remain runtime dependencies. Pending steering still begins in an in-memory queue, and external side effects need care: if a tool succeeds and the process crashes before its response is persisted, rebuilding a state machine does not magically make that tool safe to run twice. The useful change is that these boundaries are now visible instead of being mixed into one long-running coroutine.

The result is a codebase that is easier to test, refactor and experiment with. A new idea for changing an agent can be expressed as an Operation and added to a pipeline without changing the state-machine core. The original agent loop got goose surprisingly far. Now we can change how an agent behaves without adding another branch to it.
