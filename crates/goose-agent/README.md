# goose-agent

The GDK's agent loop, unrolled into a state machine you assemble yourself.

Instead of a fixed "call the model, run the tools, repeat" loop, an agent here is
an ordered list of steps. Each step gets a chance to look at the conversation and
either decline or produce effects. The machine walks the list, applies the first
step that applies, and starts over from the top — so the whole agent's behavior
is a function of the persisted conversation, not of in-memory loop state.

## The pieces

- **`Operation<S, E>`** — one step. Implement `run` to act on the conversation,
  plus any of `inference_tools`, `prompt_parts`, and `moim_parts` to contribute
  to the model request. Returns `OperationResult::NotApplicable` to pass, or
  `applied(..)` / `yielded(..)` to take the step. Helpers: `not_applicable()`,
  `applied()`, `yielded()`, `yielded_with()`.
- **`Inference<S, E>`** — the step that reaches the provider. Before calling it,
  the machine collects tools and prompt parts from *every* operation in the list
  into an `InferenceInput`. It runs whenever the conversation ends on a user or
  tool message, even if other tool calls from the same reply still have no
  response. A pipeline that keeps calls open, for example while waiting for
  the user to approve one, needs an operation before inference that yields
  while any call is open.
- **`ToolOperation<S>`** — adapts caller-defined `rmcp` tools into one operation.
  Register `SyncTool<S>` and `AsyncTool<S>` implementations with the typed builder
  methods, or implement `ToolProvider<S>` for tools discovered dynamically per
  session. Dynamic providers are queried before inference and again before
  execution, so their tool names and handlers must remain stable between those
  boundaries. Their schemas are sent to inference and matching calls are
  dispatched against the current session.
- **`StopHookOperation<S>`** — runs a `StopHook` when the model gives its final
  answer; see [Hooks](#hooks).
- **`StateMachine<'a, S, E>`** — holds `Vec<Step<..>>` and a `CancellationToken`.
  `step()` runs one pass and returns a `StepResult` whose `applied_step` names
  the step that applied (`None` if none did) and whose `status` says whether the
  run is `Continuing`, `Yielded` to the client, or `Cancelled`; `apply()` writes
  effects back; `run()` loops while `status` is `Continuing`.
- **`ConversationEffect`** — the default effect type: `AppendMessage`,
  `ReplaceConversation`, `PatchToolRequestMeta`, `SetMessageVisibility`. Bring
  your own by implementing `MachineEffect`.
- **`Emitter`** — streams `AgentEvent`s (`Message`, `Usage`, `MessageUsage`,
  `McpNotification`, `HistoryReplaced`) to the client while a step runs, and
  carries the cancellation token.
- **`SessionLoader`** / **`EffectHandler`** / **`MachineSession`** — the traits
  your runtime implements so the machine can load a session by id and persist
  effects. The machine reloads the session between passes; it never caches it.

## Hooks

A stop hook looks at the model's final answer and can send the model back to
work, for example when tests still fail. The final answer is a reply with no
tool calls.

To add one, implement `StopHook<S>` and give it to `StopHookOperation`. Its
`on_stop` returns one of:

- `Allow` — the model stops.
- `Deny` — the model keeps working. It gets `reason` as a new message, which
  the user doesn't see. If you set `user_message`, it's sent to the client to
  show the user. It isn't added to the conversation.
- An error — the run ends with that error. If you'd rather allow or deny when
  something goes wrong, handle the error inside `on_stop`.

Good to know:

- `StopHookOperation` takes one hook. To run several, write one hook that calls
  the others.
- Put `StopHookOperation` after any other step that also looks at the final
  answer. When the model stops, the steps after it don't run.
- A hook can send the model back at most 8 times per user message, so it can't
  keep the model working forever. Change the limit with
  `with_deny_limit_per_turn`. To tell the user why the model stopped at the
  limit, set a message with `with_deny_limit_message`; it is shown and saved.

Two operations decide whether each tool call may run. Register both, in this
order, before every step that runs tools:

```rust
ToolApprovalOperation::new(approval)
PreToolCallHookOperation::new().with_hook(hook)
```

`ToolApprovalOperation` decides whether a call may run and can ask the user
first. Implement `ToolApproval<S>`. Its `check` gets every new call from the
model's reply at once, so it can check them together, and returns one decision
per call, in the same order. Each decision is one of:

- `Allow` — the call is approved and goes on to the pre-tool hook.
- `Deny` — the call doesn't run. The model gets `reason` as the call's error
  result.
- `Ask` — the user is asked to confirm first. The confirmation request shows
  the tool and its arguments, plus `prompt` if you set one.
- An error — the run ends with that error. The run also ends with an error if
  `check` returns a different number of decisions than calls.

When the user chooses, `on_confirmation` gets the choice once, so approval can
remember "always allow" or "always deny". If the user declines, the call gets
the declined message as its error result; change that text with
`with_declined_message`. If the user allows, the call is approved. To approve
every call, give it an approval whose `check` always returns `Allow`.

`PreToolCallHookOperation` runs the pre-tool hook on approved calls. It never
runs for a call that is denied or declined, so it is the place for checks with
side effects, such as running user scripts. Implement `PreToolCallHook<S>`. Its
`before_call` returns one of:

- `Allow` — the call runs.
- `Deny` — the call doesn't run. The model gets `reason` as its error result.
- An error — the run ends with that error.

It handles one call per pass, so each result is saved before the next hook
runs. Without a hook, it lets every approved call run.

Good to know:

- Register both operations. Without `PreToolCallHookOperation`, approved calls
  never run; without `ToolApprovalOperation`, calls run without approval or the
  hook.
- Put `UnhandledToolCallsOperation` after every step that runs tools. Allowed
  calls run while others wait for the user.
- Show the confirmation request to the user only after it is saved, so the
  user can't choose before it exists. The operation saves it but doesn't send it.
- When the user chooses, save `confirmation_response_message(call_id,
  permission)`. It is hidden, so it isn't mistaken for a new user message.
- Calls with arguments that can't be read skip approval and the hook; the tool
  step responds to them with the error.
- If you write your own step that runs tools, run only calls that have no
  response yet and where `request_executable(request)` isn't `Some(false)`.
  `Some(false)` means the call isn't ready: it waits for the user or the
  pre-tool hook, or it was declined and already has its error result.
  `ToolOperation` already does this.
- Each operation takes one approval or one hook. To run several, write one
  that calls the others.

A post-tool hook sees each tool call's result right after the call. It can't
change the result. To add one, implement `PostToolCallHook<S>` and give it to the
step that runs the tools: `ToolOperation::new().with_post_tool_call_hook(..)`.
Its `after_call` is called once per call that ran, right after that call and
before the next one starts. `ToolOperation` logs an error and the run goes on, so
the result is still saved. If you write your own step that runs tools, call
`after_call` after each call yourself and decide what an error means; otherwise
its calls get no post-tool hook.

`UnhandledToolCallsOperation` is needed with `ToolApprovalOperation`. Put it
after every step that runs tools, right before inference. It gives calls no
step handled a "not handled" error result, and it pauses the run while calls
wait for the user, so the model isn't called with calls still open. A step that runs tools after it gets "not handled" for its
calls.

## Reading the conversation

Because steps re-derive their decisions from history, the crate ships the
predicates they need: `messages_since_kickoff`, `last_effective_role`,
`assistant_turn_count`, `ends_turn`, and `trailing_error`. When a step must
remember that it already did something, it records that on the message itself via
`Operation::set_message_meta` / `message_meta` rather than in memory.

## Cancellation

On Stop, the machine drops the running step's future, including inference
preparation. It then saves the interrupted step's `finalize_cancellation`
effects, responds to every tool request since kickoff that still has no response
with "Tool call was interrupted before completing", and finally calls
`finalize_cancellation` on the remaining operations in pipeline order. Every
operation's `finalize_cancellation` runs once, and the session is reloaded
after each save. Callers driving `step` and `apply`
themselves must call `finalize` on exit.

An operation implements `finalize_cancellation` only when it holds received
output across a later await, or owns work that must finish. Keep that output in
the operation, not in locals of the dropped future, and drain the same field on
normal completion. `finalize_cancellation` emits only what the client has not
seen yet.
`InferenceRunner` holds its streamed messages and usage; `ToolOperation` holds
its tool response message. Use one operation instance per active execution.

`Emitter` sends on an unbounded channel, so emitting never pauses and an
operation that only builds, emits, and returns holds nothing. Dropping a future
cannot undo external actions or stop blocking work that has already started.

## Skills

`skills` provides argument substitution and loaded-skill formatting. On native
platforms, it also discovers skills from explicit, ordered roots. Configuration
paths, built-in skills, and tool registration remain host responsibilities;
these helpers do not automatically change the agent pipeline. Filesystem
discovery is not a secure file-open API.

## WebAssembly

The crate builds for `wasm32-unknown-unknown` and runs without a tokio runtime,
so it can be embedded in a JavaScript host. There, the clock and randomness come
from JavaScript. Some things differ from native:

- The traits you implement (`Provider`, `Operation`, `ToolProvider`, `StopHook`,
  `SessionLoader`, `EffectHandler`, and the rest) do not require `Send` or
  `Sync`, and neither does `MessageStream`. So you can await JavaScript promises
  and hold host handles directly. Implement the traits with
  `#[async_trait(?Send)]`. The bounds are `MaybeSend` and `MaybeSync` from
  `goose_provider_types::maybe_send`, which are the real `Send` and `Sync`
  natively.
- `SyncTool`s run inline instead of on a blocking thread, so cancellation cannot
  interrupt one that has started.
- `ProviderRetry` waits between attempts on the host's `setTimeout` instead of
  tokio's timer, so retries work without a tokio runtime.

The reference assembly of these pieces is `goose::agents::state_machine` in the
[`goose`](../goose) crate.

## Connected MCP tools (native)

`mcp::McpTools::discover` takes already-connected clients under explicit namespaces
and produces an immutable tool/routing snapshot. An `rmcp::Peer<RoleClient>` can be
used directly; applications with an existing client implement the two-method
`McpToolClient<S>` interface instead. The adapter implements `ToolProvider<S>` and
can be registered with `ToolOperation::with_provider`, including its post-tool
hook and the pipeline's separate approval/pre-tool operations.

Discovery follows pagination and fails on discovery errors, repeated cursors, or
colliding public tool names. Public names are provider-safe; dispatch retains the
original server names and preserves arguments, request metadata, and results.
Discovery is explicit: retain a snapshot for the session and inference that used
it, and build a new one for subsequent work when the tool list changes. Do not
share a session-specific snapshot across sessions with different tool visibility.

`McpToolCatalog<T>` is the lower-level catalog for applications that retain their
own execution pipeline. Goose's extension leases use it for immutable routing and
provider-safe names while retaining application-specific dispatch and policy.

This module does not connect transports, launch processes, resolve configuration,
store sessions, authorize users, or own server shutdown. The caller retains the
running MCP service and configures its notification/elicitation handlers. The
basic peer adapter does not drive input-required/MRTR follow-up rounds, forward
request-scoped notifications to agent events, or automatically refresh catalogs.
