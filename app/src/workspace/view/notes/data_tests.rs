use chrono::NaiveDate;

use super::NotesData;

fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, d).unwrap()
}

fn active_texts(data: &NotesData) -> Vec<&str> {
    data.active().map(|task| task.title.as_str()).collect()
}

#[test]
fn add_trims_and_skips_blank_text() {
    let mut data = NotesData::default();
    assert_eq!(data.add("  write spec  "), Some(0));
    assert_eq!(data.add("   "), None);
    assert_eq!(data.add("review PR"), Some(1));
    assert_eq!(active_texts(&data), ["write spec", "review PR"]);
}

#[test]
fn ids_are_not_reused_after_delete() {
    let mut data = NotesData::default();
    let first = data.add("a").unwrap();
    assert!(data.delete(first));
    assert_eq!(data.add("b"), Some(first + 1));
    assert!(!data.delete(first));
}

#[test]
fn toggle_done_records_the_day_and_reopens() {
    let mut data = NotesData::default();
    let id = data.add("a").unwrap();
    assert!(data.toggle_done(id, day(16)));
    assert_eq!(data.task(id).unwrap().done_on, Some(day(16)));
    assert!(data.toggle_done(id, day(17)));
    assert_eq!(data.task(id).unwrap().done_on, None);
    assert!(!data.toggle_done(99, day(16)));
}

#[test]
fn set_title_ignores_blank_and_unchanged_titles() {
    let mut data = NotesData::default();
    let id = data.add("a").unwrap();
    assert!(!data.set_title(id, "  "));
    assert!(!data.set_title(id, " a "));
    assert!(data.set_title(id, " b "));
    assert_eq!(data.task(id).unwrap().title, "b");
}

#[test]
fn set_description_reports_only_real_changes() {
    let mut data = NotesData::default();
    let id = data.add("a").unwrap();
    assert_eq!(data.task(id).unwrap().description, "");
    let markdown = "Notes\n\n```rust\nfn main() {}\n```\n";
    assert!(data.set_description(id, markdown));
    assert!(!data.set_description(id, markdown));
    assert_eq!(data.task(id).unwrap().description, markdown);
    assert!(!data.set_description(99, "x"));
}

#[test]
fn loads_tasks_saved_before_descriptions_existed() {
    let json = r#"{"next_id":1,"tasks":[{"id":0,"text":"old","done_on":null,"archived":false}]}"#;
    let data: NotesData = serde_json::from_str(json).unwrap();
    let task = data.task(0).unwrap();
    assert_eq!(task.title, "old");
    assert_eq!(task.description, "");
}

#[test]
fn archive_done_moves_only_finished_tasks() {
    let mut data = NotesData::default();
    let done = data.add("done").unwrap();
    data.add("open");
    data.toggle_done(done, day(16));

    assert_eq!(data.archive_done(), 1);
    assert_eq!(active_texts(&data), ["open"]);
    assert_eq!(data.archived_count(), 1);
    assert_eq!(data.archive_done(), 0);
}

#[test]
fn restore_returns_a_task_to_the_active_list_in_place() {
    let mut data = NotesData::default();
    let first = data.add("first").unwrap();
    data.add("second");
    data.toggle_done(first, day(16));
    data.archive_done();

    assert!(data.set_archived(first, false));
    assert_eq!(active_texts(&data), ["first", "second"]);
    assert!(data.task(first).unwrap().is_done());
}

#[test]
fn archived_groups_newest_day_first_with_open_tasks_last() {
    let mut data = NotesData::default();
    let old = data.add("old").unwrap();
    let open = data.add("open").unwrap();
    let new_a = data.add("new a").unwrap();
    let new_b = data.add("new b").unwrap();
    data.toggle_done(old, day(14));
    data.toggle_done(new_a, day(16));
    data.toggle_done(new_b, day(16));
    data.archive_done();
    data.set_archived(open, true);

    let groups: Vec<(Option<NaiveDate>, Vec<&str>)> = data
        .archived_by_day()
        .into_iter()
        .map(|(day, tasks)| (day, tasks.iter().map(|t| t.title.as_str()).collect()))
        .collect();
    assert_eq!(
        groups,
        [
            (Some(day(16)), vec!["new a", "new b"]),
            (Some(day(14)), vec!["old"]),
            (None, vec!["open"]),
        ]
    );
}

#[test]
fn round_trips_through_json() {
    let mut data = NotesData::default();
    let id = data.add("a").unwrap();
    data.add("b");
    data.toggle_done(id, day(16));
    data.set_description(id, "- [ ] sub-step\n");
    data.archive_done();

    let json = serde_json::to_string(&data).unwrap();
    assert_eq!(serde_json::from_str::<NotesData>(&json).unwrap(), data);
}
