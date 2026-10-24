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
  tool message, even if other tool calls from the same reply are still
  unanswered. A pipeline that keeps calls open, for example while waiting for
  the user to approve one, needs an operation before inference that yields
  while any call is open.
- **`ToolOperation<S>`** — adapts caller-defined `rmcp` tools into one operation.
  Register `SyncTool<S>` and `AsyncTool<S>` implementations with the typed builder
  methods, or implement `ToolProvider<S>` for tools discovered dynamically per
  session. Dynamic providers are queried before inference and again before
  execution, so their tool names and handlers must remain stable between those
  boundaries. Their schemas are sent to inference and matching calls are
  dispatched against the current session. Pass `BeforeToolCallHook<S>`s to
  `with_before_call_hooks` to check each call before it runs, and
  `AfterToolCallHook<S>`s to `with_after_call_hooks` to check its result. A
  `Deny` before the call skips it; a `Deny` after the call discards the result.
  Either way the model gets the deny's `reason` as the tool's error. An
  after-call deny can't undo the call; if the tool has side effects, say in the
  `reason` that it already ran so the model doesn't call it again. Unparseable
  calls get no hooks, and denied calls get no after-call hooks. A
  result is saved only after its after-call hooks finish, so stopping the run
  during them saves nothing for that call. Only `ToolOperation` runs these
  hooks; a custom tool operation has to call them itself.
- **`StopHookOperation<S>`** — runs `StopHook<S>`s when the model ends its turn.
  A `Deny` keeps the turn going and sends its `reason` to the model as a hidden
  user message. Denials are capped per turn (`with_deny_limit_per_turn`,
  default 8); past the cap the turn ends and a message from the
  `with_deny_limit_message` function, if set, is built, shown, and saved.
- **`Hook<S, E>`** / **`HookDecision`** — the trait every hook implements. A
  hook returns `Allow` or `Deny { reason, user_message }`. Hooks are asked in
  order and the first `Deny` wins; later hooks are not asked. A deny's
  `user_message` is a `Message` the hook builds in whatever form the app shows;
  if present, it is shown to the user but not saved.
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

## Reading the conversation

Because steps re-derive their decisions from history, the crate ships the
predicates they need: `messages_since_kickoff`, `last_effective_role`,
`assistant_turn_count`, `ends_turn`, and `trailing_error`. When a step must
remember that it already did something, it records that on the message itself via
`Operation::set_message_meta` / `message_meta` rather than in memory.

## Cancellation

On Stop, the machine drops the running step's future, including inference
preparation. It then saves the interrupted step's `finalize_cancellation`
effects, answers every tool request since kickoff that still has no response
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

- The traits you implement (`Provider`, `Operation`, `ToolProvider`, `Hook`,
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
