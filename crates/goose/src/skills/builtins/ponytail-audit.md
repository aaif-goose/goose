---
name: ponytail-audit
description: >
 Audit the entire repo for over-engineering — not just the current diff. Finds
 what to delete across the whole codebase: unnecessary abstractions, unused
 flexibility, hand-rolled stdlib, over-engineered patterns. Use when the user
 says "audit this repo for over-engineering", "find what's over-engineered",
 "what can we delete", or invokes /ponytail-audit.
---

Audit every file touched by the task scope for things that can be deleted or
shrunk. Same format as ponytail-review but across files, not just the diff.

Audit by walking the repo — or the relevant subtree — with code-search tools.
For every file in the scope, check for:

- `<tag>: unrequested abstraction with one implementation, caller, or test
- Hand-rolled stdlib: collections.Counter, datetime, pathlib, itertools, functools, re, json, dataclasses, attrs, os.walk/shutil, difflib
- No-op dependencies: imported but never used, or replaceable with 1 stdlib line
- Boilerplate config: YAML/TOML file with 0 users, factory with 1 product, adapter with 1 backend, transformer with no transforms
- Dead argument: parameter with one caller that always passes the same value
- Speculative flexibility: enum variant nobody creates, branch nobody takes

Same format as ponytail-review: one line per finding.

`<file>:L<line>: <tag> <what>. <replacement>.`

Tags: `delete:`, `stdlib:`, `native:`, `reuse:`, `yagni:`, `shrink:`

End with: `net: -<N> lines possible across <N> files.`

Boundaries match ponytail-review: correctness, security, and performance are
out of scope.