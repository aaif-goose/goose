---
title: Operations
description: Compose agent behavior from operations, with a worked example and an inventory of existing implementations.
displayed_sidebar: gdkSidebar
---

# Operations

An operation is a unit of agent behavior. It looks at the current session and
conversation, decides whether it has work to do, and returns effects or yields
control to the caller. The state machine decides when to ask it; the runtime
decides how its effects are stored.

This is broader than a tool. A tool is a capability the model can request, such
as reading a file. An operation decides how the agent proceeds: execute a
pending tool call, require approval, add project context, compact the history,
or stop after a budget is reached. Some operations also provide tools or prompt
content to inference.

## The operation contract

Implement `Operation<S, E>` for your session type `S` and effect type `E`. Its
main entry point, `run`, receives the loaded session, conversation, and emitter.
The result controls what the machine does next:

| Result helper           | Meaning                                                                       |
| ----------------------- | ----------------------------------------------------------------------------- |
| `not_applicable()`      | This operation has nothing to do; try the next step.                          |
| `applied(effects)`      | Apply these effects, reload the session, and start again from the first step. |
| `yielded()`             | Return control to the caller without adding effects.                          |
| `yielded_with(effects)` | Save these effects, then return control to the caller.                        |

Operations can implement `inference_tools` to advertise tools, `prompt_parts`
to contribute system-prompt sections, and `moim_parts` for context the request
preparer can place near the current turn. These contributions are collected
before inference, independently of which operation last applied. The request
preparer decides how to use them.

An `Inference` also implements `Operation`, but is placed in `Step::Inference`
so the machine calls its inference methods and gathers those contributions.
An ordinary operation does not have to call a model at all.

## Worked example: a turn budget

Suppose an application should allow at most three assistant turns for each
user request. Once the current tool results have been saved, it should hand
control back to the user instead of making another model call.

We can implement this without changing inference or the state machine. This
native Rust operation works with any session and any effect type that can
represent a `Message`:

```rust
use anyhow::Result;
use async_trait::async_trait;
use goose_agent::operation::{
    assistant_turn_count, ends_turn, messages_since_kickoff,
    not_applicable, yielded_with, Emitter, Operation, OperationResult,
};
use goose_provider_types::conversation::{message::Message, Conversation};

struct TurnLimit {
    max_turns: u32,
}

#[async_trait]
impl<S, E> Operation<S, E> for TurnLimit
where
    S: Send + Sync,
    E: From<Message> + Send + 'static,
{
    fn name(&self) -> &'static str {
        "turn_limit"
    }

    async fn run(
        &self,
        _session: &S,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let messages = messages_since_kickoff(conversation)?;
        if ends_turn(messages) || assistant_turn_count(messages) < self.max_turns {
            return not_applicable();
        }

        let message = Message::assistant()
            .with_text("I've reached this request's turn limit. Would you like me to continue?");
        yielded_with([emit.message(message).into()])
    }
}
```

`messages_since_kickoff` selects the current user request and the messages that
follow it. `assistant_turn_count` counts assistant message blocks, rather than
individual streamed chunks. The operation declines when the agent has already
finished naturally or is still within budget. Otherwise, it emits a visible
message and returns that same message as an effect to be saved before yielding.

Add it after the tool operation and before inference in your assembly:

```rust
let machine = StateMachine::new(
    vec![
        Step::Operation(Arc::new(tools)),
        Step::Operation(Arc::new(TurnLimit { max_turns: 3 })),
        Step::Inference(Arc::new(inference)),
    ],
    cancel,
);
```

Here `tools`, `inference`, and the runtime come from your agent assembly, as in
[Hello World](/docs/gdk/hello-world). The ordering is intentional: a tool request
from the third assistant turn still gets a response. After that response is
saved, the budget operation yields before a fourth inference call can start.
Putting the limit before tools would stop earlier but could leave unanswered
tool calls that need separate handling.

When the user sends a new message, it becomes the next kickoff and the count
starts again. No mutable counter needs to survive a restart. This is a limit
on assistant turns, not a dollar, token, or tool-call budget. The complete goose
application has its own `MaxTurnsOperation` with additional prompt context.

## Operations available today

### Reusable GDK components

These are public APIs in `goose-agent`; your application supplies the required
tools, provider, or subagent runner:

| Component                                                                                                    | Purpose                                                                                                                        |
| ------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------ |
| [`ToolOperation`](https://docs.rs/goose-agent/latest/goose_agent/tool/struct.ToolOperation.html)             | Advertise and execute typed synchronous or asynchronous tools, or tools supplied dynamically by a `ToolProvider`.              |
| [`SubagentOperation`](https://docs.rs/goose-agent/latest/goose_agent/subagent/struct.SubagentOperation.html) | Run requested subagent sessions through your `SubagentRunner`, deliver their updates, and handle concurrency and cancellation. |
| [`InferenceRunner`](https://docs.rs/goose-agent/latest/goose_agent/inference/struct.InferenceRunner.html)    | Prepare provider requests, stream inference, and return message and usage effects. Register it as an inference step.           |

### Operations in goose

goose composes a larger agent from the same interfaces. The following are
implementations in the goose application, **not stable, reusable GDK APIs**.
They depend on goose's session, configuration, extensions, and hooks. Use them
as examples of the behavior an operation can own, not as an import list for
your application.

| Operation                     | Purpose                                                                                           |
| ----------------------------- | ------------------------------------------------------------------------------------------------- |
| `ToolApprovalOperation`       | Inspect pending tool requests and record whether they can execute or need approval.               |
| `ToolExecutionOperation`      | Execute eligible extension tool calls, including tool hooks and streamed output.                  |
| `RemainingToolCallsOperation` | Answer declined, invalid, or unhandled calls, and yield while approval is pending.                |
| `SkillOperation`              | Discover and load skills, contribute their context, and handle skill commands.                    |
| `RecipeOperation`             | Apply recipe instructions and tools, handle recipe commands, and collect structured final output. |
| `SteerOperation`              | Incorporate queued user guidance between model and tool turns.                                    |
| `BangShellOperation`          | Turn a user message beginning with `!` into a direct shell tool request.                          |
| `CompactionOperation`         | Compact conversation history automatically or through a command.                                  |
| `ToolPairCompactionOperation` | Summarize older tool request/response pairs to reduce context size.                               |
| `DoctorOperation`             | Run session diagnostics and provide repair context through `/doctor`.                             |
| `ProjectOperation`            | Contribute the selected project's instructions to the model request.                              |
| `RetryOperation`              | Evaluate recipe success checks and goal/grind continuation or retry behavior.                     |
| `EmptyResponseOperation`      | Make an empty-response fallback visible when another operation has not taken over.                |
| `StopHookOperation`           | Let configured stop hooks accept or block the end of a turn.                                      |
| `ExitOnErrorOperation`        | Yield when an error remains at the end of the conversation.                                       |
| `MaxTurnsOperation`           | Enforce goose's autonomous turn budget and contribute remaining-budget context.                   |
| `AutoEffortOperation`         | Select and record thinking effort for a turn when automatic effort selection is enabled.          |
| `SlashCommandOperation`       | Route a recognized slash command to operations' command handlers.                                 |
| `StatusOperation`             | Report the current session, model, and usage through `/status`.                                   |

The [state machine builder](https://github.com/aaif-goose/goose/blob/main/crates/goose/src/agents/state_machine_builder.rs)
shows the main composition and ordering. The
[operation modules](https://github.com/aaif-goose/goose/tree/main/crates/goose/src/agents/state_machine)
contain the implementations; some operations are only active with the relevant
configuration or are invoked through command handlers.

## How and why to write your own

Write an operation when the application needs a rule or lifecycle step that
should not depend on the model choosing the right tool. Examples include
enforcing a spending limit, waiting for an external event, validating a result,
or adding domain-specific context. If you only need a new capability that the
model may call, add a tool to `ToolOperation` instead.

Start with the condition under which your operation applies. Make its effects
change that condition, or yield, so it cannot keep selecting itself without
progress. Choose its position relative to the other steps: an approval gate
must precede tool execution, while our budget example deliberately follows it.

Keep durable decisions in the session, conversation metadata, or another store
loaded by the runtime. `set_message_meta` and `message_meta` can help mark what
an operation has already done. If a step holds received output across an await,
implement `finalize_cancellation` to return output that has not yet been saved.
Cancellation does not roll back external actions.

For a larger example, [GooseBusters](/docs/gdk/demos/goosebusters) uses custom
operations to coordinate a phone call. The full contract and result types are
in [`goose-agent::operation`](https://docs.rs/goose-agent/latest/goose_agent/operation/).
