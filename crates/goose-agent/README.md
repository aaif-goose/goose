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

## Tool approval

`ToolApprovalOperation` decides whether each tool call may run and can ask the
user first. Put it before every step that runs tools. Implement
`ToolApproval<S>`. Its `check` gets every new call from the model's reply at
once, so it can check them together, and returns one decision per call, in the
same order. Each decision is one of:

- `Allow` — the call may run.
- `Deny` — the call doesn't run. The model gets `reason` as the call's error
  result.
- `Ask` — the user is asked to confirm first. The confirmation request shows
  the tool and its arguments, plus `prompt` if you set one.
- An error — the run ends with that error. The run also ends with an error if
  `check` returns a different number of decisions than calls.

When the user chooses, `on_confirmation` gets the choice once, so approval can
remember "always allow" or "always deny". If the user declines, the call gets
the declined message as its error result; change that text with
`with_declined_message`. If the user allows, the call may run. To let every
call run without asking, register a `ToolApproval` whose `check` always returns
`Allow`. Without `ToolApprovalOperation`, no call runs.

Good to know:

- Put `UnhandledToolCallsOperation` after every step that runs tools. Allowed
  calls run while others wait for the user.
- Show the confirmation request to the user only after it is saved, so the
  user can't choose before it exists. The operation saves it but doesn't send it.
- When the user chooses, save `confirmation_response_message(call_id,
  permission)`. It is hidden, so it isn't mistaken for a new user message.
- Calls with arguments that can't be read skip approval; the tool step responds
  to them with the error.
- If you write your own step that runs tools, run only calls that have no
  response yet and where `request_executable(request)` is `Some(true)`, and
  respond to calls with arguments that can't be read with the error.
  `ToolOperation` already does this.
- `ToolApprovalOperation` takes one approval. To run several, write one that
  calls the others.

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

## Turn limits

`max_turns::MaxTurnsOperation::new(limit)` limits autonomous turns using the persisted conversation. Place it after tool execution, pending approvals, and normal completion, before inference. It counts consecutive assistant-message blocks since the latest kickoff; provider retries and history transformations can affect how closely that approximates inference calls. A new user prompt resets the budget, and a zero limit yields immediately. A completed assistant answer is preserved even at the limit.

At exhaustion, the operation appends a user-visible `ResourceExhausted` error and yields. The operation also contributes a `moim_parts` budget reminder once half the limit is used. Hosts must render these parts in their request preparer; `IdentityInferenceRequestPreparer` does not include them.

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

- The traits you implement (`Provider`, `Operation`, `ToolProvider`,
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
