# Exploratory implementation: #12758

Issue: https://github.com/aaif-goose/goose/issues/12758

The issue is Accepted / design, not Ready. This is a local exploratory slice, not an upstream-ready implementation or an agreed design.

## Direction

ACP is an agent protocol, not a completion provider. An application-level `AgentBackend` should distinguish `Standard(Arc<dyn Provider>)` from `Acp(Arc<AcpProvider>)`. Do not implement the standard Provider trait on this enum. Standard-only operations should receive a standard provider explicitly; shared application operations can dispatch without pretending ACP accepts Goose system prompts or tool schemas.

## First slice

Separate ACP adapter definitions and registration from ProviderDef. Preserve the existing runtime conversion at the registry boundary until consumers migrate. This is scaffolding, not completion: AcpProvider still implements Provider in this slice.

## Remaining migration

1. Return a typed backend from registry construction and providers::init; migrate CLI, server, and ACP server factories. Inventory and model metadata are shared discovery concerns, not runtime capabilities.
2. Replace agents::types::SharedProvider storage and agent update/access APIs. Keep standard-only helpers typed as Arc<dyn Provider> rather than adding a universal compatibility escape hatch.
3. Split execution at the application boundary. Standard inference receives prepared system prompts, history, and tools. ACP submits protocol prompts to its own session; its agent owns execution and context.
4. Implement equivalent routing in both agents/agent.rs and agents/state_machine/. Audit reply preparation, compaction, structured output, tool-pair summarization, permissions, and summon/subagents. Existing manages_own_context and permission_routing checks identify important boundaries.
5. Move useful ACP trait methods to inherent protocol-facing APIs; remove impl Provider for AcpProvider and the temporary registry conversion. Handoff memo budgeting currently uses the provider-based context-limit helper and also needs migration.
6. Decide separately whether agent-specific capabilities on the published GDK Provider trait should be removed; do not silently break GDK APIs during application wiring.

## Proposed full-change verification

- Factory tests preserve runtime variants, working directories, inventory, and default-model behavior.
- ACP tests cover resume, model/mode/effort selection, permissions, cancellation, and handoff without implementing Provider.
- Matching legacy/state-machine tests cover ACP routing, context ownership, structured-output rejection, and permissions, alongside unchanged standard inference/compaction.
- Exercise CLI and desktop standard/ACP switching and session restoration.
- Run relevant builds, tests, formatting, and clippy before an upstream PR; link a Ready issue and record agreed verification results.

No build/test runs were requested for this exploratory slice.
