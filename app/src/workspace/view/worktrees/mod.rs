//! Worktrees panel: a flowchart of a repository's git worktrees under the branches they were
//! created from, with each worktree's status and the Claude Code agents working in it.
//!
//! - [`data`]   `git worktree list` / reflog / session-file parsing (pure) and async loading.
//! - [`tree`]   arranges worktrees into the flowchart's rows (pure).
//! - [`gutter`] draws a row's connector lines and node.
//! - [`view`]   the panel.

pub(crate) mod data;
mod gutter;
pub(crate) mod tree;
pub(crate) mod view;

pub(crate) use view::{WorktreesEvent, WorktreesView};
