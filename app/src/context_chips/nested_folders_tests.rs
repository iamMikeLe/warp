use super::*;

#[test]
fn join_builds_relative_paths() {
    assert_eq!(join("", "src"), "src");
    assert_eq!(join("src", "components"), "src/components");
}

#[test]
fn anchor_ids_are_unique_per_panel_and_row() {
    assert_ne!(anchor_id(1, 2), anchor_id(2, 1));
    assert_eq!(anchor_id(1, 0), "NestedFolder-1-0");
}

#[test]
fn hovering_without_a_source_opens_nothing() {
    let mut folders = NestedFolders::default();
    assert!(matches!(
        folders.hover(0, "src".into(), "row".into()),
        HoverOutcome::Unchanged
    ));
    assert!(folders.panels.is_empty());
}

#[test]
fn listings_fill_open_panels_and_are_cached() {
    let mut folders = NestedFolders::default();
    folders.loaded("src".into(), vec!["a".into(), "b".into()]);
    // Nothing was open, but the listing is cached for the next hover.
    assert!(folders.panels.is_empty());
    assert_eq!(folders.cache.get("src").map(Vec::len), Some(2));
    assert_eq!(folders.folder_in_panel(1, 0), None);
}
