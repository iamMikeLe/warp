//! The app-wide Notes list, shared by every window's panel and persisted to `notes.json` in the
//! Warp data directory.

use std::path::{Path, PathBuf};

use chrono::Local;
use warpui::{Entity, ModelContext, SingletonEntity};

use super::data::{NotesData, TaskId};

const FILE_NAME: &str = "notes.json";

pub(crate) enum NotesEvent {
    Changed,
}

pub(crate) struct NotesModel {
    data: NotesData,
    path: PathBuf,
    /// Set when the file exists but couldn't be read; saving is then disabled so the user's
    /// notes are never overwritten with an empty list.
    load_error: Option<String>,
    save_error: Option<String>,
}

impl NotesModel {
    pub fn load() -> Self {
        Self::load_from(warp_core::paths::data_dir().join(FILE_NAME))
    }

    pub fn load_from(path: PathBuf) -> Self {
        let (data, load_error) = match read(&path) {
            Ok(data) => (data, None),
            Err(err) => {
                log::warn!("Couldn't read notes from {}: {err}", path.display());
                (NotesData::default(), Some(err))
            }
        };
        Self {
            data,
            path,
            load_error,
            save_error: None,
        }
    }

    pub fn data(&self) -> &NotesData {
        &self.data
    }

    /// A problem the panel should show: the file couldn't be read, or the last save failed.
    pub fn error(&self) -> Option<String> {
        if let Some(err) = &self.load_error {
            return Some(format!(
                "Couldn't read {}: {err}. Changes won't be saved until the file is fixed or \
                 removed.",
                self.path.display()
            ));
        }
        self.save_error
            .as_ref()
            .map(|err| format!("Couldn't save notes: {err}"))
    }

    pub fn add(&mut self, text: &str, ctx: &mut ModelContext<Self>) {
        let added = self.data.add(text).is_some();
        self.commit(added, ctx);
    }

    pub fn toggle_done(&mut self, id: TaskId, ctx: &mut ModelContext<Self>) {
        let changed = self.data.toggle_done(id, Local::now().date_naive());
        self.commit(changed, ctx);
    }

    pub fn set_title(&mut self, id: TaskId, title: &str, ctx: &mut ModelContext<Self>) {
        let changed = self.data.set_title(id, title);
        self.commit(changed, ctx);
    }

    pub fn set_description(&mut self, id: TaskId, description: &str, ctx: &mut ModelContext<Self>) {
        let changed = self.data.set_description(id, description);
        self.commit(changed, ctx);
    }

    pub fn delete(&mut self, id: TaskId, ctx: &mut ModelContext<Self>) {
        let changed = self.data.delete(id);
        self.commit(changed, ctx);
    }

    pub fn set_archived(&mut self, id: TaskId, archived: bool, ctx: &mut ModelContext<Self>) {
        let changed = self.data.set_archived(id, archived);
        self.commit(changed, ctx);
    }

    pub fn archive_done(&mut self, ctx: &mut ModelContext<Self>) {
        let changed = self.data.archive_done() > 0;
        self.commit(changed, ctx);
    }

    fn commit(&mut self, changed: bool, ctx: &mut ModelContext<Self>) {
        if !changed {
            return;
        }
        if self.load_error.is_none() {
            self.save_error = write(&self.path, &self.data).err();
            if let Some(err) = &self.save_error {
                log::warn!("Couldn't save notes to {}: {err}", self.path.display());
            }
        }
        ctx.emit(NotesEvent::Changed);
    }
}

fn read(path: &Path) -> Result<NotesData, String> {
    match std::fs::read_to_string(path) {
        Ok(json) => serde_json::from_str(&json).map_err(|err| err.to_string()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(NotesData::default()),
        Err(err) => Err(err.to_string()),
    }
}

/// Writes through a temporary file and a rename so a crash mid-write can't truncate the notes.
/// The file is plain text of at most a few hundred kilobytes, so writing it on the main thread on
/// every edit stays cheap.
fn write(path: &Path, data: &NotesData) -> Result<(), String> {
    let json = serde_json::to_string_pretty(data).map_err(|err| err.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|err| err.to_string())?;
    std::fs::rename(&tmp, path).map_err(|err| err.to_string())
}

impl Entity for NotesModel {
    type Event = NotesEvent;
}

impl SingletonEntity for NotesModel {}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
