# plan

Move the reusable MCP runtime into `goose-agent`: a scoped connection manager, immutable tool-catalog leases, and an `McpOperation<S>` that advertises and executes MCP tools. Keep goose as the host that resolves configuration, persists selections, supplies authentication and interaction handlers, and constructs application-specific servers.

Do **not** move goose's `ExtensionConfig`, platform registry, session manager, or OAuth implementation wholesale. Preserve the useful lifecycle semantics, not the application's dependency graph. Deliver this in phases, with goose using the same GDK manager and operation—not a second, parallel MCP stack.

## What exists today

Research based on the current checkout:

- [`goose-agent`](crates/goose-agent/README.md) already has `Operation<S, E>`, inference contributions (tools, prompt parts, MOIM), approval/pre/post-tool hooks, cancellation finalization, and native/Wasm support. `ToolOperation` supports dynamic `ToolProvider`s, but asks providers again at execution; this alone does not preserve MCP connection/catalog identity across inference and approval.
- [`extension_manager`](crates/goose/src/agents/extension_manager/mod.rs) combines two concerns: reusable scoped connection slots and host policy. Slots start lazily and once, reuse compatible connections, and track configuration and capability changes. Selection and enable/disable also read and write goose sessions and global configuration.
- [`ExtensionLease`](crates/goose/src/agents/extension_manager/lease.rs) pins connections and a tool catalog. Selection and tool-list changes affect subsequent leases, not a held catalog. It handles naming, filtering, pagination, resources, prompts, server instructions, MOIM, dispatch, notifications, and app-specific calls. A held lease survives scope release.
- [`mcp_client.rs`](crates/goose/src/agents/mcp_client.rs) provides the client abstraction, initialization, roots, form elicitation, notifications, timeouts, and protocol cancellation. It is coupled to goose action-required handling and tool context. Sampling is deliberately not advertised.
- Connectors support stdio, streamable HTTP (including Unix sockets and OAuth scope escalation), built-in servers over duplex IO, and direct platform clients. Stdio startup also includes goose malware checks, executable search paths, subprocess handling, and container placement.
- [`ToolExecutionOperation`](crates/goose/src/agents/state_machine/ops_toolcalling.rs), [`inference_preparation`](crates/goose/src/agents/state_machine/inference_preparation.rs), and the [pipeline builder](crates/goose/src/agents/state_machine_builder.rs) coordinate the lease used for inference and execution. The operation also contains goose-only hooks, hints, chat-mode behavior, slash commands, extension mutations, and UI handling. Missing original leases are treated conservatively for approved calls.

The important existing abstraction is the **lease**, not the configuration enum. This is an extraction and boundary cleanup, not just wrapping `call_tool` in an operation.

## Proposed boundary

| `goose-agent` owns | Host/goose owns |
|---|---|
| MCP client interface and transport-independent connection setup | User configuration, environment/secret interpolation, keyring and defaults |
| Scoped registrations, lazy startup, reuse, load reports, release | Durable enabled-extension selection and transaction semantics |
| Lease/catalog identity, namespacing, ownership, allowlists | Platform registry, built-in factories, skills/recipes/code-mode policy |
| Tools, resources, prompts, instructions, notifications, pagination | Registry discovery, installation, package malware screening, executable lookup |
| Tool execution operation, cancellation, timeout mechanics | Browser OAuth flows, refresh/credential persistence, scope-escalation UX |
| Explicit host callbacks for roots and elicitation | Session directories, container launch commands, interaction UI |
| Optional native stdio and HTTP conveniences | MCP Apps hydration/trust annotations, goose result mutations and telemetry |

No GDK dependency on `goose`, global config, database, filesystem-based credentials, or automatic browser interaction. No hidden default servers or management tools.

### 1. Connection manager and explicit registrations

Introduce a `goose_agent::mcp` module with a manager, client interface, registration/connection context, and lease. Use MCP protocol types from `rmcp`; avoid another public schema mirroring the protocol.

- A registration has a stable server ID, explicit revision, connector/client factory, tool exposure policy, and resolved connection options. The connector returns an initialized client and owns its underlying transport lifecycle. Support attaching an already-connected client as well as lazy factories.
- Native conveniences accept resolved command/args/env/cwd or URL/headers/HTTP options. Generic transport attachment supports duplex/in-process servers and host-provided transports. A direct client adapter supports existing platform extensions.
- The host maps its session to an opaque scope ID and supplies the desired registration set. GDK does not infer a session schema or load a session by ID.
- Reconcile registrations atomically per scope; reject duplicate server IDs. Reuse only when revision and relevant placement/capability context match. Explicit invalidation replaces global-config-generation checks. Revision changes cover credential changes without putting secrets into public IDs or logs.
- Keep single-flight startup, per-server load reports, and partial availability. A failed slot remains failed until explicit retry/invalidation or a new revision; do not silently spawn on every inference.
- Releasing a scope drops the manager's ownership, not held leases. Offer explicit asynchronous shutdown/drain; cancellation of a run must not terminate a connection still shared by another lease. Define ownership when attaching borrowed/shared clients so GDK does not shut down a host-owned service.

### 2. Lease safety and operation integration

`McpOperation<S>` takes a manager and a small session-to-scope/registration adapter; no `S: goose::Session` requirement. Implement using `ConversationEffect` convertible into the caller's effect type, following existing GDK operations.

The operation contributes tools and server instructions, forwards MCP notifications through `Emitter`, executes only its own outstanding executable requests, and invokes the existing post-tool hook. Register approval and pre-tool-hook operations before it, and unhandled-call handling after all tool executors.

**Pin the advertised catalog through execution and approval.** Resolve one lease for an inference boundary and share it across tools, instructions, inspection/approval, and dispatch. Pin the public name → server ID → original server tool name mapping, schema, and exposure policy. `tools/list_changed` invalidates future catalogs only. Never re-resolve an approved call against today's registration set.

The manager may be shared across sessions; operation execution state must be per active run. Provide an explicit run binding rather than an incidental shared `Mutex<Option<Lease>>`. Before implementing, settle how inference preparation acquires this binding and how multiple inference rounds replace it only after outstanding calls complete. A narrow GDK inference-preparation/binding hook is preferable to making every operation own MCP state or extending `ToolProvider` with all MCP concepts.

Record a non-secret binding identifier alongside advertised tool requests/approval state. If a pipeline is rebuilt without the original binding, fail the pending call and ask for a new request/approval; durable reconnection does not prove the original authorization still applies. Connection/catalog identity is runtime state, not serialized transport state. Preserve the existing conservative behavior during migration.

Cancellation must both notify the MCP peer and save already-received results/action messages exactly once. Token cancellation alone is insufficient when the machine drops the operation future: use a request-lifetime guard or independently owned cancellable task. Drain operation-owned buffers in `finalize_cancellation`; do not retry interrupted side effects automatically.

**Why a dedicated operation:** a plain dynamic `ToolProvider` would lose pinned catalogs, streaming notifications, elicitation, instructions, and cancellation details. Share small tool-execution helpers with `ToolOperation` where useful, but defer a grand unified executor until both implementations demonstrate a need.

### 3. Protocol surface and host integration

| Capability | Intended outcome |
|---|---|
| Tool discovery/calls | Full pagination, annotations/meta/structured results, allowlists, deterministic naming and ordering |
| Resources and prompts | Lease APIs with pagination and explicit owner; host decides what enters context or becomes a tool/command |
| Instructions | Operation prompt contributions; host description/prompt formatting remains customizable |
| Dynamic tools | Versioned cache invalidation; new lease sees updates, current lease stays pinned |
| Notifications/progress/logs | Connection and request correlation; bounded/best-effort telemetry must not block tool results |
| Roots | Explicit host-provided workspace roots; directory changes update/recreate appropriate connection context |
| Form elicitation | Awaitable host handler with request/scope identity, cancellation and timeout; capability advertised only when installed |
| HTTP auth | Host-provided authenticated connector/client; preserve refresh and step-up behavior in goose adapter |
| MCP Apps | Preserve protocol metadata and capability negotiation; goose retains app visibility enforcement, owner checks and resource hydration |
| MOIM | Optional client adapter contribution; not required for ordinary MCP clients |
| Sampling | Out of scope; do not advertise unsupported capability |

This is near-complete **extension runtime parity**, not moving every goose feature into GDK. Discovery/installation, persistent configuration, automatic OAuth, goose management tools, built-in server implementations, and UI presentation are intentional host features, not missing transport functionality.

External results must not gain host authority during the split. Preserve stripping/validation of goose mutation metadata, trusted app annotations, and subagent-session metadata in the goose adapter. App-originated calls stay bound to the owning server and app-visible tools; never authorize from flattened names alone.

### 4. Dependencies and compatibility

Keep transport-independent APIs compatible with GDK's `MaybeSend`/`MaybeSync` model. Native stdio/process and HTTP conveniences must be optional and target-gated; enabling MCP must not break the existing Wasm core build. Initial Wasm support can be host-supplied clients/transports; do not promise browser networking or stdio parity.

Use `rmcp`'s existing transport/auth primitives rather than porting OAuth into GDK. Keep redirect/credential-forwarding protections when moving HTTP helpers. Resolve the minimum feature layout and TLS selection in phase 1; prevent ordinary agent consumers from inheriting process, HTTP, OAuth, and platform dependencies by default.

Preserve goose's current public tool names and owner metadata during migration, including unprefixed platform tools and provider-safe name conversion. New GDK registrations should default to explicit namespacing and reject post-conversion collisions instead of silently choosing the first server. Keep legacy name recovery and collision behavior only in a documented goose compatibility adapter initially; changing these must be deliberate, not accidental fallout from extraction.

## Phased delivery

Two required phases, organized around the deliverables: make MCP support available through GDK operations, then make goose use it. Policy separation is implementation sequencing within phase 1, not a standalone refactor milestone. Necessary cleanup is part of completing each phase.

### Phase 1 — GDK MCP support

Separate configuration/session resolution from connection/catalog logic while extracting the reusable core into `goose-agent::mcp`. Introduce resolved registrations, connector/client contracts, and interaction handlers. Adapt goose's existing manager to delegate to the extracted runtime as we go, rather than building a second stack or completing a substantial goose-only refactor first.

Implement the dedicated MCP operation and explicit run binding. Provide host-client/transport attachment and optional native stdio/streamable HTTP connectors. Expose resources/prompts APIs and host handlers for roots/elicitation. Separate result trust/Apps decoration and selection mutations from generic dispatch; keep these in goose adapters.

Internal checkpoints (not separate delivery phases):

1. Agree the host boundary, inference/run binding, and dependency/feature layout before publishing APIs.
2. Extract the manager/client/lease core and route goose's existing manager through it, preserving lifecycle and concurrency behavior.
3. Add operation integration and a standalone in-memory-session example with two MCP servers, no goose/config/OAuth persistence dependencies.

**Exit:** a standalone GDK agent advertises, approves, executes, streams and cancels calls against pinned catalogs; add/remove/reconfigure takes effect at the next boundary. The reusable core has no global config/session/OAuth-store access, goose exercises the same connection runtime, and the Wasm core build is preserved. goose may temporarily retain its existing executor until phase 2.

### Phase 2 — goose adoption

Use the GDK operation in the existing pipeline and finish migrating all consumers to the GDK manager/lease APIs. Translate `ExtensionConfig` to resolved registrations. Keep a thin goose extension service for selection persistence, factories, config invalidation, OAuth, container startup, and client capabilities. Bridge elicitation to `ActionRequiredManager`; retain goose hooks and result decoration through explicit adapters.

Split subdirectory hints, prompt slash commands, chat-mode restrictions and special skill/code-mode visibility out of generic MCP execution. Migrate CLI, ACP session changes, subagents, recipes, extension-manager tools, and app-originated calls. Providers managing their own tool loops must continue excluding duplicate local MCP execution. Preserve transactional selection/mutation behavior on partial failures.

**Exit:** goose has one MCP runtime and executor, and all current connector types work through it, including authenticated HTTP with refresh/step-up and Unix sockets. Remove the superseded manager/client/execution implementations and unused migration shims; do not leave permanent delegation through both stacks. The requested GDK capability and goose adoption are complete at this point.

### Optional follow-up simplifications

Consider unifying `Builtin`/`Platform` configuration semantics behind host factories (existing code already overlaps them), replacing silent tool collisions with errors, and removing permissive name recovery. These are independent behavior changes with migration notes, not a third required phase or a place to defer necessary cleanup. Do not add new legacy extension-directory functionality.

## Verification plan for implementation

Port existing manager tests as behavioral contracts, particularly concurrent first resolve/enable, resolve-during-enable, pinned selection/tool-list changes, changed directory/client negotiation, partial startup failures, scope release with held leases, and invalidation during a tool-list fetch.

Add GDK integration fixtures for:

- Two servers and mixed local/MCP operations; correct approval/pre/post-hook ordering and no double dispatch.
- Removal/replacement/schema changes while awaiting approval; reconstruction without a lease must not execute against a new server.
- Multi-page tools/resources/prompts, naming collisions, allowlists, structured results, instructions and dynamic notifications.
- Cancellation/timeouts during initialization, a call, elicitation, and after receiving a result; protocol cancellation, no lost/duplicate persisted messages, process/task cleanup.
- Roots and capability negotiation, per-scope notification/elicitation routing, and overlapping sessions without cross-talk.
- goose adapters: OAuth refresh/step-up, custom headers and redirects, Unix sockets, containers/built-ins, management mutations, code mode, skills, subagents and app ownership/trust protections.

Run affected crate tests, format/clippy checks, native/Wasm feature checks, and goose self-test coverage during implementation. No builds/tests are needed for this design-only change.

## Decisions to review

1. Approve manager + dedicated operation in `goose-agent`, with goose configuration/auth/installation deliberately outside it?
2. Approve pinned run bindings and fail-closed reconstruction rather than transparent rebinding of approved calls?
3. Approve near-complete parity via host adapters, with browser OAuth, Apps UX and platform servers remaining goose-owned?
4. Approve namespaced/collision-rejecting GDK defaults while goose preserves legacy naming initially?
5. Approve standalone native MCP as the first milestone, host-client attachment on Wasm, and full goose cutover as the completion criterion?

## Documentation verification

Public behavior and architectural boundaries were checked against [GDK reference](https://goose-docs.ai/docs/gdk/reference/index), [using extensions](https://goose-docs.ai/docs/getting-started/using-extensions), [MCP roots](https://goose-docs.ai/docs/guides/mcp-roots), and [MCP elicitation](https://goose-docs.ai/docs/guides/mcp-elicitation). Internal lifecycle details are grounded in the source links above; proposed APIs and changes are explicitly design choices, not documented existing functionality.

## Implementation notes

The two implementation commits introduce standalone GDK MCP support and migrate
goose execution to `McpOperation` with a `GooseMcpHost` adapter. Both paths share
scoped ownership/reconciliation, lazy startup, catalogs, schema normalization,
and protocol-request cancellation. goose's extension service still owns config
invalidation, durable selection, connection construction, prompt commands,
notification/action-required bridging and result trust policy. Its existing
lease remains a host wrapper rather than being replaced with the standalone
GDK `Lease`; `Client<C>` supports its richer per-call context.

Standalone bindings are explicit: keep a lease while approvals are pending;
`Lease::new_binding` creates a fresh catalog/binding on the same connections for a
subsequent inference boundary. This does not yet provide an automatic dynamic
registration operation or automatic background catalog refresh. Hosts drive
that boundary. Explicit async shutdown/drain remains follow-up work; scope
release removes ownership and held leases/host-owned clients retain theirs.
Native transport convenience features must not be enabled on Wasm; the core
and host-supplied clients compile for Wasm, but all-features Wasm builds do not.
These are remaining differences from the full proposed surface, not completed
parity claims.

Verification: GDK tests and native feature clippy pass; the Wasm core compiles.
goose's 156 state-machine, 43 extension-manager and 17 MCP-client tests pass.
Affected-crate clippy and a CLI build without local inference pass; code-mode
and tree-sitter compilation also passes. Workspace clippy is blocked by an
existing llama-cpp/OpenSSL C++ compile failure. The updated live self-test recipe
was attempted but timed out without output, so it is not counted as passed.

### Consolidation follow-up

Removed the parallel client contracts and the empty goose compatibility trait.
`Client<C>` now serves both paths, with protocol dispatch implemented once through
`RequestSender<C>`. goose uses `TransportClient<GooseClient>` for transport startup,
server information and request/cancellation ownership; it retains request metadata,
active-call tracking and modern-protocol special cases. Paginated discovery and
tool exposure are shared, including resource/prompt pagination in goose.

The former standalone manager wrapper and scoped runtime are now one generic
`mcp::host::Manager<T>`, and both leases use a pinned `Selection<T>`. This reduces duplicate
ownership types but does not finish the originally proposed concrete registration
migration: goose still supplies its own startup slots, reuse/config validation,
and app-aware lease wrapper. Converting those to resolved GDK registrations is
remaining work, not implied by the generic manager consolidation.
