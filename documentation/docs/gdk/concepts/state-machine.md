---
title: State machine
description: Compose operations into an agent whose next action is determined by its loaded state.
displayed_sidebar: gdkSidebar
---

import StateMachineDiagram from "@site/static/img/gdk/state-machine.svg";

# State machine

An agent often starts as a loop: call a model, run the tools it requests, and
repeat. That works until the application also needs approvals, retries,
compaction, external events, or a way to stop and resume. Those concerns need
places in the execution order, not just more instructions in a prompt.

GDK represents execution as an ordered list of steps. You compose a
`StateMachine` from [operations](/docs/gdk/concepts/operations) and inference
steps. Changing the agent means changing that composition or writing an
operation, not extending the state machine itself.

## The execution cycle

For each pass, the machine:

1. Loads the session through your `SessionLoader`.
2. Tries the steps in order until one applies.
3. Passes that step's effects to your `EffectHandler`.
4. Starts again with the updated session, or returns control to the caller.

A step can decline, apply effects and continue, or apply effects and yield.
If no step applies, the machine yields. A step is not a program counter: the
next pass starts at the top of the list, not at the following step.

<StateMachineDiagram />

A simple composition looks like this. `tools` and `inference` are instances
configured by your application, using the same session and effect types:

```rust
let machine = StateMachine::new(
    vec![
        Step::Operation(Arc::new(tools)),
        Step::Inference(Arc::new(inference)),
    ],
    cancel,
);
```

After a user message, the tool operation has nothing to do and inference runs.
If the model requests a tool, that request becomes part of the conversation.
On the next pass, the tool operation executes it and returns its result as an
effect. The following pass reaches inference with that result in context.
When the model finishes without further work, the inference step declines and
the machine returns control to the application.

Before inference, the machine also asks the operations for their tool
definitions and prompt contributions. A component can therefore influence the
model request even when its `run` method has no action to take.

## State belongs to the application

`MachineSession` exposes the session ID and conversation. Your session type
can also hold application data, such as a working directory, approval state,
or a business record. Effects describe changes to apply: appending a message,
replacing a compacted history, or updating application-specific state.

The machine does not prescribe a database. You implement loading and applying
effects against files, a database, or an in-memory store. `Emitter` events are
separate: they let a client display output while work is running, but displaying
an event does not persist its effects.

Because decisions are made from loaded state, a fresh machine can continue a
saved conversation without remembering which branch of a loop it was in.
Operations must record decisions that matter to subsequent runs rather than
keep them only in local variables. The
[Hello World session store](/docs/gdk/hello-world#own-the-session-and-its-effects)
shows a file-backed version of this boundary.

## Yielding and resuming

Yielding means returning control to the caller. It can mean the agent has
finished, needs human approval, or must wait for an external event. It does not
start a background worker or automatically schedule a later run.

Your application persists the new input or decision, constructs the appropriate
operations, and invokes the machine again. For example, an approval operation
can yield while a tool request is awaiting a decision, then allow execution
after that decision has been saved. It must appear before any step that would
execute the unapproved request or infer past it.

Persistence alone does not make external actions exactly-once. If a process
stops between making a payment or sending a message and saving the result,
the application still needs a way to reconcile that action. Store identifiers
and use idempotent external APIs where the workflow requires them.

## Running, stepping, and stopping

Use `run` to let the machine load, step, and apply until it yields or is
cancelled:

```rust
let session = machine.run(&runtime, &session_id, &emitter).await?;
```

Applications that need tighter control can drive `step` and `apply` themselves.
The returned `StepResult` identifies the step that applied and whether the run
is continuing, yielded, or cancelled. Callers driving those methods themselves
must also call `finalize` when leaving the run.

Cancellation drops the running step's future, then gives operations a chance
to preserve received output and finalize interrupted tool responses. `run`
handles that finalization before returning. This cannot undo external side
effects or guarantee that work started outside the async task has stopped.

Next, read [Operations](/docs/gdk/concepts/operations) to see how to compose a
policy into this cycle. The full API is in
[`goose-agent::machine`](https://docs.rs/goose-agent/latest/goose_agent/machine/).
