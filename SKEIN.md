# Skein

*A context-driven, eval-first agentic quality engineering distro of goose.*

> A *skein* is a group of geese in flight. The metaphor maps directly to Skein's architecture: a coordinated flock of subagents — Spec, Dev, Test, Review, Red-team — flying in formation behind a lead goose, the human tester.

Skein is an internal-tooling distribution of [goose](https://github.com/block/goose) tailored for **professional test engineers**. It is built as a custom distro on top of upstream goose (see [CUSTOM_DISTROS.md](CUSTOM_DISTROS.md)), so we keep merging from upstream cheaply and adopt new goose features as they land.

> **New here?** Start with **[GETTING_STARTED.md](GETTING_STARTED.md)** — a 10-minute walkthrough from a fresh checkout to your first AI-assisted exploratory test charter on a real feature brief.

## What Skein is

- A **research assistant for skilled testers** — proposes charters, suggests oracles, runs investigations the tester directs. Not a verdict-issuing bot.
- A **multi-agent quality council** — Spec / Dev / Test / Review / Red-team subagents whose **disagreements are the product**, not noise to suppress.
- A **test bench for LLM-using systems** — multi-conversation, latency-aware, timeout-aware, MCP-aware, Locust-driven SLA probing.
- An **integrator** — Promptfoo, Langfuse, Playwright (Java + Python), AIO Tests, Locust wired in as MCP extensions, never duplicated.
- An **eval-first** product — every recipe ships with reference tasks, a failure-mode taxonomy, an L1/L2/L3 grader ladder, calibration records, and pass^k metrics.

## What Skein is *not*

- A test-case generator that promises to replace QA.
- A CI bot that issues verdicts on PRs.
- Another coding copilot with a testing tab.

## Doctrine

Three converging sources shape how Skein recipes are built and shipped:

- **Context-driven testing** (Bach, Bolton, Winteringham) — testing is a skilled human activity; Skein augments testers, it does not replace them. Heuristics and oracles are first-class concepts.
- **Anthropic — *Demystifying Evals for AI Agents*** — multi-grader composition (code-based + model-based + human), capability vs. regression suites, pass@k vs. pass^k, transcript review as non-negotiable, balanced (two-sided) tasks, "grade outcomes not paths."
- **Adam Mahdi — *Measuring What Matters*** — every metric must change a decision; no vanity metrics; multi-dimensional, stakeholder-aware.
- **Hamel Husain on LLM evaluation** — *look at your data first*, failure-mode taxonomies precede graders, L1/L2/L3 grader ladder (assertions → human → LLM-judge), production traces feed back into eval suites, "evals are the product."

A new recipe is not approved for ship until **50 production-or-realistic traces have been hand-reviewed and a `failure-modes.yaml` exists**. Graders come *after* the data.

## Architecture

Skein is a *distro* of goose, not a fork of its core. Our customizations live in additive locations so that upstream goose merges remain low-friction:

| Concern | Lives in |
|---|---|
| Recipes (charters, oracles, triage, investigation, council) | `recipes/` |
| Eval framework (tasks schema, graders, run_kpass, calibration) | `eval-bench/` |
| MCP bridges (Promptfoo, Langfuse, AIO Tests, Locust, screenshot-diff) | `crates/goose-mcp/` (additive) and `mcp-bridges/` |
| Branding | `ui/goose2/` (strings, splash, about) |
| Security gating | `recipe-scanner/` (already present in this fork) |
| CI auth | `oidc-proxy/` (already present in this fork) |

We **do not** rename or restructure the core `goose`, `goose-cli`, `goose-server`, `goose-mcp`, `goose-sdk`, etc. crates. Their identities stay as upstream so `git merge upstream/main` stays clean.

## Roadmap

The full seven-phase roadmap (Phase 0 Foundation → Phase 6 Quality Council) plus the 35 product ideas, the eval methodology, the rituals, and the verification metrics are tracked in the planning document used to bootstrap this distro. The headline phases:

- **Phase 0 — Foundation, Identity, Eval Backbone** (3–4 weeks). Brand, security gating, `eval-bench/`, Trace Inspector, Annotation Queue, Failure-Mode Taxonomy view, Slice Explorer, Langfuse Bridge.
- **Phase 1 — Tester's Co-Pilot v1** ← MVP. Charter Composer (SFDIPOT), Oracle Composer (FEW HICCUPPS), Triage Failure, Bug Advocacy. Multi-driver Playwright codegen (Java first, Python next).
- **Phase 2 — LLM Infrastructure Testing.** Locust Bridge, LLM SLA Prober, Multi-Conversation Runner, MCP-under-Load, Conversation Mutation Testing, LLM SLA dashboard.
- **Phase 3 — Investigation Mode.** Re-runs and fingerprinting; "flake = question to investigate," not "thing to delete." Visual Diff Oracle. Field Failure → Eval Task loop.
- **Phase 4 — CI/CD-native Mode + AIO Write-back.** GitHub Actions templates via `oidc-proxy/`. Java + Python CI stitching. AIO Tests Bridge v2 (write-back).
- **Phase 5 — Quality Intelligence.** Test Impact Analysis, Risk Heatmap, Coverage Gap Hunter, Quality Observatory.
- **Phase 6 — Quality Council.** Spec / Dev / Test / Review / Red-team multi-agent recipe with disagreement as first-class signal.

## Anti-patterns we explicitly avoid

- Forking core agent code. Customizations are additive.
- Building a backend service. Local SQLite for results stores.
- Reinventing Promptfoo / Langfuse / Playwright / AIO Tests / Locust. Wrap them as MCP extensions.
- Vitest-only or Playwright-JS-only test outputs. Java + Python first.
- Auto-issuing verdicts on PRs. The tester decides.
- Vanity metrics on dashboards (no "tests generated per minute," no token counts, no run counters).
- One-sided evals. For every positive case, ship the negative complement.
- Grading paths instead of outcomes.
- Marketing "AI replaces QA." Skein augments skilled testers; if a Bach-tradition tester would call our framing dishonest, we change it.

## Upstream merge cadence

We rebase from `block/goose` weekly. Skein customizations live in additive paths to keep merges painless. The `upstream` git remote is configured fetch-only on this clone to prevent accidental pushes to the public goose repository.

```bash
git fetch upstream
git merge upstream/main          # on a working branch
# resolve, run tests, ship
```

## Status

Phase 0 is in progress. See [SKEIN_STATUS.md](SKEIN_STATUS.md) for current implementation state and per-phase deliverables.
