use super::*;

fn ids(views: &[ToolPanelView]) -> Vec<&'static str> {
    views.iter().map(ToolPanelView::stable_id).collect()
}

#[test]
fn saved_order_wins_and_unlisted_panels_follow_in_default_order() {
    let default = vec![
        ToolPanelView::ProjectExplorer,
        ToolPanelView::ConversationListView,
        ToolPanelView::GlobalSearch {
            entry_focus: GlobalSearchEntryFocus::Results,
        },
        ToolPanelView::WarpDrive,
    ];

    let mut views = default.clone();
    sort_by_saved_order(&mut views, |view| view.stable_id(), &[]);
    assert_eq!(ids(&views), ids(&default));

    let saved = ["warp_drive", "unknown_panel", "global_search"].map(String::from);
    let mut views = default.clone();
    sort_by_saved_order(&mut views, |view| view.stable_id(), &saved);
    assert_eq!(
        ids(&views),
        [
            "warp_drive",
            "global_search",
            "project_explorer",
            "conversations"
        ]
    );
}
