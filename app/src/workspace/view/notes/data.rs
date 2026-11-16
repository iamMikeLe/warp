//! The Notes todo list: tasks on a running list that are ticked off, then archived.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

pub(crate) type TaskId = u64;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Task {
    pub id: TaskId,
    pub text: String,
    /// The local date the task was ticked off; `None` while it is still open.
    pub done_on: Option<NaiveDate>,
    pub archived: bool,
}

impl Task {
    pub fn is_done(&self) -> bool {
        self.done_on.is_some()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct NotesData {
    next_id: TaskId,
    /// Active and archived tasks, each kept in the order it was added.
    tasks: Vec<Task>,
}

impl NotesData {
    /// Appends a task unless `text` is blank; returns the new task's id.
    pub fn add(&mut self, text: &str) -> Option<TaskId> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.tasks.push(Task {
            id,
            text: text.to_string(),
            done_on: None,
            archived: false,
        });
        Some(id)
    }

    /// Ticks the task off on `today`, or reopens it if it is already done.
    pub fn toggle_done(&mut self, id: TaskId, today: NaiveDate) -> bool {
        self.with_task(id, |task| {
            task.done_on = match task.done_on {
                Some(_) => None,
                None => Some(today),
            };
        })
    }

    /// Replaces the task's text; a blank `text` leaves it unchanged.
    pub fn edit(&mut self, id: TaskId, text: &str) -> bool {
        let text = text.trim();
        if text.is_empty() {
            return false;
        }
        self.with_task(id, |task| task.text = text.to_string())
    }

    pub fn delete(&mut self, id: TaskId) -> bool {
        let before = self.tasks.len();
        self.tasks.retain(|task| task.id != id);
        self.tasks.len() != before
    }

    pub fn set_archived(&mut self, id: TaskId, archived: bool) -> bool {
        self.with_task(id, |task| task.archived = archived)
    }

    /// Archives every finished task on the active list; returns how many moved.
    pub fn archive_done(&mut self) -> usize {
        let mut moved = 0;
        for task in self
            .tasks
            .iter_mut()
            .filter(|task| !task.archived && task.is_done())
        {
            task.archived = true;
            moved += 1;
        }
        moved
    }

    pub fn active(&self) -> impl Iterator<Item = &Task> {
        self.tasks.iter().filter(|task| !task.archived)
    }

    /// Archived tasks grouped by the day they were finished, newest day first. Tasks archived
    /// while still open come last, under `None`.
    pub fn archived_by_day(&self) -> Vec<(Option<NaiveDate>, Vec<&Task>)> {
        let mut groups: Vec<(Option<NaiveDate>, Vec<&Task>)> = Vec::new();
        for task in self.tasks.iter().filter(|task| task.archived) {
            match groups.iter_mut().find(|(day, _)| *day == task.done_on) {
                Some((_, tasks)) => tasks.push(task),
                None => groups.push((task.done_on, vec![task])),
            }
        }
        // `None` sorts below every date, so reversing puts it last.
        groups.sort_by(|(a, _), (b, _)| b.cmp(a));
        groups
    }

    pub fn archived_count(&self) -> usize {
        self.tasks.iter().filter(|task| task.archived).count()
    }

    pub fn ids(&self) -> impl Iterator<Item = TaskId> + '_ {
        self.tasks.iter().map(|task| task.id)
    }

    pub fn task(&self, id: TaskId) -> Option<&Task> {
        self.tasks.iter().find(|task| task.id == id)
    }

    fn with_task(&mut self, id: TaskId, update: impl FnOnce(&mut Task)) -> bool {
        match self.tasks.iter_mut().find(|task| task.id == id) {
            Some(task) => {
                update(task);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
#[path = "data_tests.rs"]
mod tests;
