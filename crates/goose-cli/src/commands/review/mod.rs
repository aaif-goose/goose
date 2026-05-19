//! `goose review` — local code review tool.
//!
//! Discovers `**/.agents/checks/*.md` subagent reviewers and `**/.agents/REVIEW.md`
//! scoped prompt overrides, builds a review request from the working tree (or an
//! explicit diff range), and dispatches the review to the configured agent.
//!
//! Modeled after Amp's `review` command.
//!
//! Check parsing and discovery live in [`goose::checks`] so they can be reused
//! from other entry points (server, ACP) without depending on this CLI.

pub mod handler;
pub mod orchestrator;
pub mod prompt;

pub use handler::{handle_review, ReviewOptions};
