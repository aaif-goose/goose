# JavaScript Operations

Build Goose with the `js-operations` feature to load custom state-machine
Operations from JavaScript files at runtime. `code-mode` and `js-operations`
independently enable the shared `js-runtime` feature and its Boa dependency.
Enabling code mode alone does not load custom Operations.

For the CLI, enable `--features js-operations`, then add file paths and placements
to your Goose configuration:

```yaml
GOOSE_JS_OPERATIONS:
  - /absolute/path/to/local-policy.js
  - path: /absolute/path/to/check-response.js
    before: stop_hook
  - path: /absolute/path/to/inspect-tools.js
    after: tool_execution
  - path: /absolute/path/to/follow-up.js
    after: js:check-response
  - path: /absolute/path/to/custom-budget.js
    replace: max_turns
```

Or set an environment variable containing a JSON array:

```sh
export GOOSE_JS_OPERATIONS='["/absolute/path/to/local-policy.js"]'
```

Relative paths resolve against the Goose process's working directory. Files are
loaded and validated when each turn's pipeline is assembled, including subagent
turns. Prompt previews also load these files. Source is pinned for that pipeline;
edits take effect the next time the pipeline is assembled. Invalid paths,
definitions, duplicate names, unknown anchors, conflicting replacements, or placement cycles fail setup
instead of silently skipping policies.

## Placement

A plain file path places the Operation immediately before `llm` (inference),
preserving the original behavior. An object must specify `path` and exactly one
of `before`, `after`, or `replace`. An anchor is the exact Operation name, including the `js:`
prefix when targeting another JavaScript Operation. Forward references to files
later in the configuration are supported.

Built-in anchors, in their normal order, are:

```text
slash_command, tool_approval, skills, recipe, tool_execution,
remaining_tool_calls, foreground_subagent, steer, bang_shell, compaction,
tool_pair_compaction, doctor, project, retry, empty_response,
stop_hook, exit_on_error, max_turns, auto_effort, llm
```

`compaction` is absent when the provider manages its own context. `auto_effort`
exists only when enabled and configured. Targeting an absent Operation fails with
a list of available anchors. `status` is a slash-command handler rather than a
pipeline step, so it cannot be used as an anchor.

Operations attached to the same side of an anchor retain configuration order.
Each Operation and its attached Operations form a group: if A is before
`stop_hook` and B is after `js:A`, B stays after A and before `stop_hook`.
Built-in Operations keep their existing relative order. Use `before: slash_command`
to run at the very beginning, or `after: llm` to place an Operation at the end.

Placement controls the machine's sequential scan, not a once-through lifecycle.
An earlier Operation can act or yield before a later Operation's `run` is reached.
Whenever an Operation applies with `continuing`, the scan restarts from the
beginning after persisting effects. In particular, `after: llm` is reached only
when inference declines on that pass; to inspect a completed response, place the
Operation before `stop_hook` and check the conversation in `run`.

Slash-command handlers are collected in the resolved pipeline order, followed by
the built-in `/status` handler. The first applicable `run_command` handles the
command. The dispatcher itself runs at `slash_command`, so Operations before it
can act first. All Operations' prompt contributions are collected in resolved
order before inference, including Operations positioned after `llm`.

Placement takes effect when the next pipeline is assembled. It does not reload or
reorder a pipeline already executing. Operations communicate through persisted
conversation effects and notes; placement does not share JavaScript globals.

## Replacing built-in Operations

`replace` removes a built-in Operation and puts the JavaScript Operation in its
slot. The built-in's execution, slash-command hooks, prompt contributions, tool
definitions, and cancellation finalizer no longer participate. The replacement
provides whichever hooks it needs; omitted hooks use the normal empty defaults.

Both the original anchor and the replacement's own `js:<name>` anchor identify
that slot. For example, if `custom-budget.js` declares `name: "custom-budget"` and
replaces `max_turns`, both entries below run after the replacement, in config order:

```yaml
GOOSE_JS_OPERATIONS:
  - path: /absolute/path/to/report-budget.js
    after: max_turns
  - path: /absolute/path/to/notify-budget.js
    after: js:custom-budget
  - path: /absolute/path/to/custom-budget.js
    replace: max_turns
```

The replacement keeps its JavaScript name for attribution and operation notes;
the original anchor is an alias for placement only. Forward references work
through either anchor, and entries attached to either name share config ordering.

Each built-in can be replaced once. Missing targets, attempts to replace another
JavaScript Operation, and multiple replacements of one built-in fail setup.
`replace: llm` is rejected because inference requires the separate `Inference`
interface. `status` cannot be replaced because it is not a pipeline step.

Replacing `slash_command` removes automatic slash-command dispatch, including
the built-in `/status` handler. The replacement's `run` hook is responsible for
command handling; its `run_command` hook is not automatically called without a
dispatcher.

## File format

Define `const operation = { name, ...hooks }`, as in
[`local-policy.js`](local-policy.js). Hooks may return values synchronously or
return promises. The supported hooks are:

| Hook | Return value when supplied | Default when absent |
| --- | --- | --- |
| `run(input)` | Operation result | Not applicable |
| `run_command(input)` | Operation result | Not applicable |
| `inference_tools(input)` | Array of MCP tool definitions | `[]` |
| `prompt_parts(input)` | Array of `[key, text]` pairs | `[]` |
| `moim_parts(input)` | Array of strings | `[]` |
| `finalize_cancellation(input)` | Array of effects | `[]` |

Every input contains `operation_name` (for example `js:local-policy`), `session`
(the serialized session without its conversation field), and
`conversation: { messages: [...] }`. `inference_tools` receives a null conversation
if the supplied session has none. `run_command` additionally receives
`command: { command, params_str }`, without the leading slash.

Names are prefixed with `js:` for machine attribution and persisted operation
notes. Keep names stable across edits. Unknown hooks and non-function hooks are
rejected. The names `run` and `__goose_input` at file scope are reserved for the
host wrapper. Use the `run` method inside the `operation` object.

## Results and effects

Return `{ kind: "not_applicable" }` to let the next Operation run. To act, return:

```javascript
{
  kind: "applied",
  status: "continuing", // or "yielded" to return control to the client
  effects: [/* effects below */],
}
```

`continuing` restarts the pipeline after effects are persisted. Ensure the
persisted conversation makes the Operation decline on a later pass, or it can
keep applying indefinitely. `effects` defaults to `[]`; `status` is required.

Supported effects use a `type` discriminator:

| Type | Fields |
| --- | --- |
| `append_message` | `message`: a serialized Goose message |
| `replace_conversation` | `messages`: an array of serialized Goose messages |
| `patch_tool_request_meta` | `tool_call_id`, `patch` |
| `set_message_operation_note` | `message_id`, `key`, `value` |
| `set_message_visibility` | `message_id`, `user_visible`, `agent_visible` |

Messages use Goose's existing JSON format; see the example for a text message.
The host generates missing IDs and publishes appended messages with the same IDs
used for persistence. Tool-confirmation messages are published after persistence.
The note effect automatically uses the current Operation's namespaced identity.
For example:

```javascript
const operation = {
  name: "mark-once",
  run({ operation_name, conversation }) {
    const message = conversation.messages.at(-1);
    if (!message || message.metadata.operations?.[operation_name]?.handled) {
      return { kind: "not_applicable" };
    }
    return {
      kind: "applied",
      status: "continuing",
      effects: [{
        type: "set_message_operation_note",
        message_id: message.id,
        key: "handled",
        value: true,
      }],
    };
  },
};
```

The entire result is decoded before any effects or message events are produced.
Malformed results fail the hook. Mutating input objects has no effect on Rust
state; persist changes through returned effects. `inference_tools` contributes
schemas only; handling corresponding tool requests remains the Operation's job.

## Execution and cancellation

Each hook runs in a fresh context with a five-second deadline, including time
waiting for one of four shared JavaScript workers. Instruction, loop, recursion,
and output limits are shared with code mode. Results must be JSON, with a maximum
depth of 64 and a 10,000-item conversion budget; console output and the serialized
result share a 1 MB limit. Console output is bounded and discarded for Operations.
These limits do not provide a hard heap-memory bound or process isolation.

No imports, packages, timers, filesystem, shell, environment, network access, or
`tools.call` binding are supplied to Operations. The source file is read by Rust;
it is not itself a JavaScript filesystem capability. Session data is supplied
explicitly, so only configure files you intend to give that data to.

On cancellation the running hook is dropped. `finalize_cancellation`, when
present, runs in a fresh context with an independent deadline and reads the
persisted session. It returns effects rather than an Operation result. Finalizer
errors are logged. JavaScript globals do not survive between hooks; use persisted
operation notes to record completed actions.

## Differences from Rust Operations

The adapter supports every Operation hook, but currently exposes a smaller set
of capabilities than a Rust implementation:

- **Host services:** JavaScript cannot directly call providers, MCP tools,
  extension management, hooks, the session store, filesystem, shell, or network.
  `inference_tools` can declare a tool and `run` can compute its response, but
  declaring a schema does not provide access to those host services.
- **Effects:** only the five conversation effects listed above are accepted.
  Rust Operations can also compact with usage accounting, set a recipe, update
  extension state, record provider usage, or use a custom effect type. Replacing
  the conversation from JavaScript does not perform the host's compaction workflow.
- **Events:** JavaScript cannot stream messages, usage, or MCP notifications
  through an `Emitter` while a hook is running. Appended messages are published
  after the hook returns a valid result. Console output is not a client event.
- **In-memory state and cancellation:** contexts are fresh for every hook, so
  JavaScript cannot retain handles, background tasks, or partially received output
  for a later hook or cancellation finalizer. It cannot inspect or cancel the
  machine's cancellation token. Finalizers can read persisted history and return
  new effects, but cannot recover locals from the interrupted execution.
- **Inference and types:** the adapter implements `Operation<Session, GooseEffect>`
  over JSON snapshots. It cannot implement the separate `Inference` interface,
  prepare a native inference session, replace `llm`, or accept arbitrary Rust
  session types, references, and service handles. JavaScript packages and imports
  are not supported by this embedding.
- **Execution budgets:** JavaScript has the deadline, instruction, recursion,
  result-size, and worker limits described above. The timeout can be changed by
  Rust embedding code, but is not currently configurable in `GOOSE_JS_OPERATIONS`.

These are limits of the current bridge. Replacing a built-in removes its host
behavior; it does not transfer that built-in's services to the JavaScript replacement.

## Embedding

Rust callers can also load a file directly and insert it into their own ordered
`StateMachine` steps:

```rust,ignore
use std::sync::Arc;
use goose::agents::js_operations::JavaScriptOperation;
use goose::agents::state_machine::Step;

let operation = JavaScriptOperation::from_file("local-policy.js").await?;
let step = Step::Operation(Arc::new(operation));
```

`JavaScriptOperation::with_timeout` changes the deadline for subsequent hooks.
The adapter implements `Operation<Session, GooseEffect>`; it does not implement
the separate `Inference` trait.

`load_operations(&config)` returns files with their placements. Embedders can use
`place_operations(steps, configured_operations)` to resolve insertions and replacements
against their own named pipeline steps.
