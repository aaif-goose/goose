## Task

A goose session is being compacted because the context window is filling up. You will read the transcript below and write a structured summary that another instance of you will read to continue the session.

This summary is your *only* memory of what happened. The original messages will not be available. Be specific. Prefer concrete facts (file paths, function names, command outputs, decisions) over generic prose.

If a fact isn't in the transcript, don't invent one. If something is uncertain, say so. The next instance is allowed — encouraged — to say "I don't have that in my context, can you re-share?" rather than guess.

**Conversation History:**
{{ messages }}

{% if read_files %}**Files already read this session (do not re-read unless they may have changed):**
{{ read_files }}
{% endif %}{% if modified_files %}**Files modified this session:**
{{ modified_files }}
{% endif %}
## Format

Write the summary using exactly these sections, in this order. Use markdown. Be concise but specific — favor bullet points over prose. Omit sections that have nothing concrete to say.

### Goal
What the user is ultimately trying to accomplish in this session.

### Constraints & Preferences
- User-stated requirements, conventions, taste calls, "always do X / never do Y" rules.
- Repo / project rules surfaced during the session (style, build, test).

### Progress
#### Done
- [x] Concrete things that are finished and verified (committed, tested, confirmed by user).

#### In Progress
- [ ] What is actively being worked on right now, with enough specificity to resume.

#### Blocked / Open Questions
- Anything waiting on the user, an external system, or an unresolved decision.

### Key Decisions
- **Decision**: short rationale. Include alternatives considered and why rejected if it came up.

### Critical Context
- File paths, function/class/symbol names that matter for the next step.
- Exact error messages, command names, env vars, URLs, IDs.
- Snippets of code, config, or output that the next instance will need to reason about. If a snippet is short (<20 lines), include it verbatim. If it is long, summarize it and cite the file path so it can be re-read.

### Next Step
The single most concrete next action. One sentence describing what to do, and which file/command/tool to use. If there's no clear next step, write "Awaiting user input" and leave it at that — do not invent work.

<read-files>
List every file path the agent has read in this session, one per line. Include any from the "Files already read" block above plus any others discovered in the transcript. Absolute paths preferred.
</read-files>

<modified-files>
List every file path the agent has written or edited in this session, one per line. Absolute paths preferred.
</modified-files>
