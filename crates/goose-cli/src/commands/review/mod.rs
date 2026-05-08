//! `goose review` — local code review tool.
//!
//! Discovers `**/.agents/checks/*.md` subagent reviewers and `**/.agents/REVIEW.md`
//! scoped prompt overrides, builds a review request from the working tree (or an
//! explicit diff range), and dispatches the review to the configured agent.
//!
//! Modeled after Amp's `review` command.

pub mod check;
pub mod discover;
pub mod handler;
pub mod prompt;

pub use handler::{handle_review, ReviewOptions};
