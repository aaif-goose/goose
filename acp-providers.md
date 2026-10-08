# ACP provider separation — session handoff

## Goal and scope

Issue: https://github.com/aaif-goose/goose/issues/12758 — **Distinguish provider types: ACP vs traditional providers**.

ACP is an agent protocol, not a completion-provider API. We should not force `AcpProvider` into `goose_providers::base::Provider`: ACP owns its session, context, permissions, and tool execution, unlike ordinary model providers. The desired distinction belongs at the Goose application layer.

The issue was **Accepted / design**, not **Ready**, when inspected at the beginning of this session. This work is local/exploratory. Before an upstream PR, recheck board status and agree on design, scope, non-goals, and verification. No issue comments or PRs were posted.

## Where the work lives

- Worktree: `/Users/jackamadeo/development/goose-12758`
- Branch: `exploratory/12758-acp-backend`
- Original/main working directory: `/Users/jackamadeo/development/goose`
- At handoff, the exploratory worktree was clean before creating this document.

### Base correction

The worktree initially inherited the unrelated `anthropic-official-thinking-caps` branch at `e915ac94d`. An attempted full rebase was aborted at the user's request and all local changes restored. We then committed only the ACP construction slice and rebased **that one commit** onto fetched `origin/main`, excluding the unrelated Anthropic commits.

This conflicted with `723a825fb` — **Add ProviderManager and route session providers through it (#12745)**, by Douwe Osinga. That change added `ProviderDef::SESSION_BOUND` and set it to true for ACP adapters. Conflict resolution preserved session-bound behavior in the dedicated ACP registration path and added a test assertion.

### Current commits

1. `e0d076eb2` — `refactor: separate ACP provider construction from standard definitions`
2. `a71faf14b` — `refactor: trim standard provider definitions to session-independent construction`
3. `5d0a710bb` — `provider backend`
4. `718788298` — `refactor: retain typed backends through agent switching and injection`

The third slice was uncommitted at the end of the prior assistant turn; inspection at handoff confirms it is now committed under the third hash above. Do not assume it is still unstaged.

## Work completed

### 1. Separate ACP construction

Added `AcpProviderDef` in `crates/goose/src/providers/base.rs`. It extends shared `ProviderDescriptor` metadata but returns concrete `AcpProvider` instances rather than imposing the standard `Provider` bound.

Migrated all five ACP definitions and registrations: Amp, Claude, Codex, Copilot, and Pi. Added `ProviderRegistry::register_acp_with_inventory`, preserving inventory resolution, constructor dispatch, working directory, default-model handling, and session isolation.

### 2. Trim standard ProviderDef

Audited the actual construction inputs before removing them:

- Ordinary providers ignored extensions; none overrode the working-directory or default-model construction hooks.
- Claude Code and Codex are non-ACP exceptions: they resolve extensions into MCP configuration and retain process/session state.
- Gemini CLI ignores extensions but retains a session ID, so remains session-bound.
- Cursor Agent is CLI-backed but was classified as stateless/shareable; it remains on the standard definition path.
- Goose mode was **not** a ProviderDef constructor argument. Claude Code reads mode from configuration during construction, and ProviderManager propagates session mode through runtime `update_mode`. These behaviors were not removed.
- No legacy non-ACP definition consumed the construction-time working-directory hook. Removing it did not change existing subprocess working-directory behavior or fix any existing limitations there.

`ProviderDef` now consists of its associated standard provider type and `from_env(tls_config)`. Removed its extension argument, working-directory/default-model hooks, and `SESSION_BOUND` constant.

Introduced `SessionBoundProviderDef` for Claude Code, Codex, and Gemini CLI, retaining extensions and TLS configuration. Dedicated registry registration sets session-bound metadata true; ordinary definitions set it false. Updated implementations, helper constructors, registrations, and affected test definitions.

### 3. Retain runtime backend identity

Added `crates/goose/src/providers/backend.rs`:

```rust
#[derive(Clone)]
pub enum ProviderBackend {
    Standard(Arc<dyn Provider>),
    Acp(Arc<AcpProvider>),
}
```

**The enum does not implement Provider.** Standard includes both ordinary shared providers and the legacy session-bound non-ACP providers; the variants distinguish protocol/backend kind, not cache eligibility.

- Registry constructors now return `ProviderBackend` and retain the correct variant for ordinary, legacy session-bound, declarative, and ACP definitions.
- ProviderEntry offers `create_backend`, `create_backend_with_working_dir`, and `create_backend_with_default_model`.
- `providers::init` exposes typed creation functions alongside compatibility APIs.
- Existing creation APIs returning `Arc<dyn Provider>` remain documented transitional wrappers.
- ProviderManager session slots now store `ProviderBackend`; it exposes `backend_for` and `set_backend`.
- Its shared cache remains standard-provider-only and explicitly matches the constructed variant, rejecting ACP rather than guessing from a provider name.
- Existing `provider_for` and `set_provider` remain compatibility APIs. `set_provider` wraps injected handles as Standard; new ACP injection should use `set_backend` to retain identity.
- Small inherent enum APIs dispatch name, model selection, and mode updates explicitly by variant.

### Tests added/updated

- ACP-only definition tests cover inventory overrides, refresh support, preferred/builtin classification, session-bound metadata, and constructor dispatch.
- Standard/session-bound registry tests cover classification and normal, working-directory, and default-model entry points.
- Typed construction tests check variant retention using an ACP test fixture without spawning a subprocess.
- ProviderManager injection tests check typed ACP storage, legacy standard injection, and release.
- Existing mocks and constructor call sites were updated for the trimmed signatures.

### 4. Typed application access, switching, and injection

Committed as `718788298`:

- Added `Agent::backend` and `update_backend`; switching, effective model configuration, mode updates, and thinking-effort updates retain typed identity. Existing `provider` and `update_provider` remain transitional APIs for unmigrated consumers and standard injection.
- Legacy thinking-effort normalization is confined to Standard. ACP raw effort values remain intact.
- ACP identity, model selection, mode updates, thinking-effort support/selection, and supported-model discovery now have inherent APIs. Trait adapters delegate to them. ACP control APIs no longer accept local goose session IDs; requests use the remote ACP session ID.
- CLI startup/backend acquisition, model-switch capability checks, and completion-cache identity are typed. Existing CLI restrictions on ACP model/provider switching are preserved. Context-window calculations remain explicitly Standard-only. The CLI's hypothetical acquisition helper still mutates the real manager slot, as before; it is not a side-effect-free preview.
- ACP server factories return typed backends. Model discovery explicitly dispatches to standard discovery or ACP session options; inventory refresh uses standard recommended models or ACP advertised models. Native OAuth remains Standard-only.
- Subagent `TaskConfig` retains `ProviderBackend` through injection. Construction uses resolved extensions and working directory. Fallback sharing permits only matching Standard providers that do not manage their own context; typed ACP and legacy-erased ACP cannot use that fallback.
- Regression tests cover typed acquisition/injection, ACP and Standard replacement, ACP raw-effort preservation and rejection, mode persistence, remote control request IDs, session isolation/release, discovery, and subagent fallback restrictions.
- No execution-loop changes or published GDK API changes were made. Doctor/completion health probes and other standard-only inference consumers remain for a later explicit capability audit. This is a refactor slice, not a new user-facing feature; the self-test recipe was not changed or run.

### 5. Explicit execution split (working tree)

This follow-up slice is implemented and verified but **not committed**:

- ACP has inherent `prompt_messages(ModelConfig, messages)` with no standard system/tool inputs. Resume, remote-session identity, context-limit resolution, permission responses, and effort subscriptions are inherent as well; existing Provider methods delegate.
- Legacy inference explicitly matches backend kind. ACP skips goose conversation repair, system/tools/toolshim preparation, project/MOIM preparation, automatic compaction, tool-pair summarization, standard model-error enhancement/retries, and completion-based session naming.
- The state machine uses a direct `AcpInferenceRunner` implementing application `Inference`/`Operation`, not a Provider adapter. Its common operations retain command/lifecycle behavior through `WithoutInferencePreparation`, without constructing standard completion inputs. Standard continues to use the existing GDK inference runner and request preparer unchanged.
- Recipe capability checks use explicit name/support data; ACP structured responses fail before inference. Status uses typed context lookup. ACP has no compaction operations. Permissions route through typed backend controls.
- ACP errors persist partial output and errors, and cannot be restarted by blocking Stop hooks. Empty responses do not cause host completion retries. Tool-only completion has an invisible terminal assistant marker so Stop hooks and continuation retain their lifecycle.
- Live ACP permission messages are not republished after persistence. Cancellation removes stale pending permissions via a guard, sends a cancelled permission outcome, and drops the stream promptly.
- Transport stream drop sends `session/cancel` for the remote ACP session and drains the original prompt before accepting the next queued prompt. A duplex integration test verifies notification delivery and isolation of late old chunks. A nonresponsive peer can still block the connection while draining; no timeout/session-teardown redesign was introduced.
- Nine regression tests exercise both loops for streaming/usage/metadata, external-tool isolation, errors, empty completion, structured-output rejection, permissions, cancellation, and tool-only completion with Stop-hook continuation.
- No GDK APIs or the ProviderBackend capability contract were expanded to standard inference. This remains exploratory; no upstream issue/PR communication.

Verification for this execution slice:

- Formatting and whitespace checks passed.
- `cargo check -p goose -p goose-cli --all-targets` passed during implementation; final workspace clippy (`cargo clippy --all-targets --features goose/scheduler -- -D warnings`) passed after all production changes and added tests.
- 337 combined ACP-provider, agent, reply-parts, and state-machine tests passed using `--features scheduler,tree-sitter`, serial execution, `RUST_MIN_STACK=16777216`, and an isolated config root with keyring disabled. The nine both-loop ACP tests also passed separately.
- ACP bootstrap/effort (3), cancellation transport (1), custom requests (21), and secret cache integration (1) tests passed: 26 integration tests.
- Running reply-parts tests alone exposed a pre-existing initialization issue: synchronous tool-categorization tests construct Agent/SessionManager without a Tokio context. They pass in the combined suite after async agent tests initialize the shared storage. No unrelated test/runtime initialization changes were made.
- No live external-agent smoke test, self-test recipe run, or CLI binary rebuild was performed for this refactoring slice.

## Important limitations / temporary bridges

This is **not yet the full issue implementation**:

- `AcpProvider` still implements `Provider` in `crates/goose/src/acp/provider.rs`.
- `ProviderBackend::into_legacy_provider` explicitly erases identity for unmigrated consumers.
- ACP identity, model/mode/effort, prompt execution, resume, context, permission, and subscription APIs are inherent. Remaining compatibility consumers still acquire erased trait handles.
- Both agent inference loops now split execution by variant. Other CLI/server, diagnostics, summarization, and platform-extension consumers still use compatibility provider APIs.
- Registry entry creation still accepts generic session inputs for compatibility, though standard definitions no longer receive those inputs.
- Existing `manages_own_context` and `permission_routing` flags still do runtime work. They are useful audit markers, not the final typed separation.

Do not implement Provider on the enum or add a general `as_provider` accessor as the final design. That would reproduce the original capability mismatch behind a new type.

## Recommended next steps

1. Typed agent access/update/switching, CLI construction, ACP server factories, and legacy summon/subagent injection are migrated. Continue auditing remaining consumers, especially doctor/health probes and capability-specific CLI/server operations.
2. Execution is now split in both loops: Standard receives goose-prepared completion inputs; ACP uses its protocol session. Maintain parity while migrating the remaining consumers.
3. Audit remaining trait erasure in CLI/server resume/model/effort operations, doctor/health checks, execute_commands, summaries, tool-call labels, security/permission inspectors, and platform extensions before removing compatibility APIs.
4. Audit preparation/compaction/structured output/tool summarization and permission routing. Relevant files include `agents/reply_parts.rs`, `agents/execute_commands.rs`, `agents/state_machine/ops_llm.rs`, `ops_compaction.rs`, and platform extension summon code.
5. Inherent ACP APIs are in place; migrate remaining consumers to them rather than adding capability forwarding to the backend enum.
6. Remove `impl Provider for AcpProvider`, then remove compatibility creation APIs and `into_legacy_provider` once callers are migrated.
7. Simplify shared registry construction inputs if useful after compatibility callers are gone; do not let factory cleanup expand into an unnecessary discovery/inventory rewrite.

## Open design questions

- What is the smallest genuinely shared application API? Identity and session controls can dispatch, but standard-only inference helpers should keep an explicit standard provider input.
- Should the legacy session-bound CLI backends remain Standard long term, gain their own backend variant, or migrate to ACP? Current code preserves them as Standard rather than claiming they behave like ordinary model APIs.
- Where should per-session extension, working-directory, and Goose-mode configuration be represented for ACP? Current adapter factories still read some global configuration; typed construction does not solve that distinction yet.
- How should mode/model updates and switching handle remote-session identity, resume, handoff, and cancellation? Avoid passing a local Goose session ID where an ACP session ID is required.
- How should ACP handoff budgeting resolve context size? `bounded_handoff_memo` currently calls a provider-based context-limit helper using `self`; that dependency must be removed with the Provider implementation.
- Which callers truly require standard capabilities (doctor/model discovery, structured output, summarization, subagents)? Choose explicit support/rejection rather than a universal trait-shaped forwarding layer.
- How should tests and old injection APIs prevent ACP handles from being accidentally wrapped as Standard? `set_provider(Arc<dyn Provider>)` cannot recover a previously erased variant.
- Should agent-specific capabilities eventually leave the published GDK Provider trait? That is a separate public-API decision: do not silently break GDK crates during application refactoring.
- Resolve issue readiness and agreed scope before treating this exploratory branch as an upstream implementation.

## Verification status and proposed completion checks

**Follow-up verification authorized and performed:**

- `cargo fmt --all -- --check` and `git diff --check` passed.
- `cargo check -p goose -p goose-cli --all-targets` passed (CLI defaults also enable feature-gated AWS/local provider definitions). An unused CLI import was subsequently removed and final clippy validated the clean result.
- `cargo clippy --all-targets --features goose/scheduler -- -D warnings` passed, including a final rerun after code changes.
- Registry: 5 tests passed; ProviderManager: 2 tests passed (`--features scheduler`).
- ACP provider: 111 tests passed (`--features scheduler`, serial).
- Agent: 53 tests passed; summon: 44 tests passed (`--features scheduler,tree-sitter`, serial).
- ACP custom requests: 21 tests passed; secret cache integration: 1 test passed (`--features scheduler,tree-sitter`, serial).
- State machine: 90 tests passed (`--features scheduler,tree-sitter`, serial, `RUST_MIN_STACK=16777216`).
- CLI session: 215 tests passed, 1 ignored, using the default-feature test binary with a temporary `GOOSE_PATH_ROOT`, `GOOSE_DISABLE_KEYRING=1`, no `GOOSE_THINKING_EFFORT`, and serial execution.

**Environment/test caveats observed:** the no-feature goose unit-test build fails in existing scheduler-gated test code; agent tests require tree-sitter for the analyze-extension test. Parallel ACP tests hung in the pre-existing handoff-retry test; that test passed alone and the full ACP suite passed serially. State-machine tests overflowed the default test-thread stack and passed with the larger stack. CLI tests under the user's global configuration hit a completion-cache timeout/keyring hang and a thinking-effort expectation mismatch; both passed in the isolated configuration. These unrelated test/environment problems were not changed. No live external ACP-agent smoke test or self-test recipe run was performed.

Before upstream submission, resolve issue readiness and execute the agreed full-change verification plan; this slice does not validate the eventual execution split.

Full-change verification should include:

- Typed factory variants, inventory, TLS forwarding, working directories, default-model behavior, and cache/session isolation.
- ACP resume, model/mode/effort selection, permissions, cancellation, and handoff without a standard Provider implementation.
- Matching legacy/state-machine tests for ACP execution, context ownership, structured-output behavior, and permissions, with unchanged standard inference and compaction.
- CLI/desktop standard-to-ACP and ACP-to-standard switching, including session restoration and release.
- Self-test recipe updates/execution if the agreed feature scope requires them.

An upstream PR should link a Ready issue and explain execution of its agreed verification plan. Existing supplementary notes: `notes/12758-acp-backend.md` (earlier first-slice plan; this handoff reflects the later enum work).
