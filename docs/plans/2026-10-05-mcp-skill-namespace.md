# Plan: namespace MCP skills by server host

**Goal:** Two MCP servers that advertise the same skill name both stay visible, addressed as `host::skill-name`, without a remote skill silently replacing a local one.

**Source:** https://github.com/aaif-goose/goose/issues/12068#issuecomment-6006702707

**Affected files:**

- `crates/goose/src/skills/mcp.rs` — qualified name, merge, load lookup
- `crates/goose/src/agents/extension_manager/lease.rs` — pass the server host, not only the extension display name
- `crates/goose/src/agents/state_machine/ops_skills.rs` — prompt line and `load_skill` accept `host::name` and `host::name/file`
- `crates/goose/src/agents/extension.rs` — read-only: `StreamableHttp.uri` is the host source

**Approach:**

Keep the current rule that a filesystem skill hides an MCP skill of the same bare name. Do not prompt to overwrite a local skill in this slice. A remote skill is not a file we can install, and a yes/no prompt inside `load_skill` has no UI path yet.

An MCP skill is always `host::name`, even when it is the only server advertising that name. Publishing the bare name and rewriting it when a second server connects would change a name the model already saw. The host is the URL host of a streamable HTTP extension (`billing.example` from `https://billing.example/mcp`). Stdio and builtin servers have no host, so they use the extension name. `::` is the separator because skill names must not contain it. A filesystem skill keeps its bare name and is not overwritten.

`load_skill(name: "billing.example::refunds")` splits on the first `::`, finds that record, and reads it from the extension that served it. `billing.example::refunds/guide.md` is the supporting file. A request for the bare name when only a qualified form exists returns the qualified names, not a silent pick.

**Edge cases:**

- Two servers on the same host (two paths, one domain) still collide. Fall back to `host/path-tail::name` only if the host alone is not unique. Do not invent this until a test needs it.
- A skill name that already contains `::` is rejected at parse time. Current name checks already reject whitespace and control characters. Add `::`.
- Prefix must not be parsed as a supporting-file path. Split `::` before `/`.
- Local skills stay bare. Never rewrite `~/.agents/skills/refunds` to a prefixed name.
- The prompt line shows the qualified name and the serving extension, so the model can tell two refunds skills apart.

**Test plan:**

- One MCP server, name free: prompt and `load_skill` still use the bare name.
- Two servers, same skill name, different hosts: both appear as `host::name`. Neither is dropped.
- A filesystem skill named `refunds` still hides both bare aliases. The qualified forms remain.
- `load_skill("billing.example::refunds/guide.md")` checks the file list before `resources/read`.
- A name containing `::` from the server is skipped, same as a name/URI mismatch.

**Conventions:**

- No `ExtensionLease` import in `crates/goose/src/skills/`. The host is a string argument, so the edvige port can pass it from the running extension set.
- No `SourceType::McpSkill`.
- No prompt that writes over a local skill directory.

**Open questions:**

- Overwrite prompt ("keep local or take remote") needs a user-visible choice. Not this slice. A later desktop prompt can copy the remote `SKILL.md` into `~/.agents/skills` only after an explicit yes.
- Same host, two connectors: defer until a real config hits it.

**Estimated size:** M
