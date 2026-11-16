//! Notes panel: a daily todo list in the tools panel.
//!
//! - [`data`]  the task list and its operations (pure).
//! - [`model`] the app-wide list, persisted to `notes.json`.
//! - [`view`]  the panel.

pub(crate) mod data;
pub(crate) mod model;
pub(crate) mod view;

pub(crate) use view::NotesView;
