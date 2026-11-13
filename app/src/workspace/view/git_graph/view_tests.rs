//! Unit tests for the pure repo-selection decision in [`super`] (the view
//! layer). These cover [`pick_selected_repo`], which decides which discovered
//! repo a tab lands on — the logic that keeps a per-tab repo choice across tab
//! switches.

use super::*;

/// Three sibling repos under a common parent; the anchor lives inside the
/// first one. `pick_selected_repo` matches a repo by `anchor.starts_with(r)`,
/// so the paths share a prefix on purpose.
fn repos() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/work/repo1"),
        PathBuf::from("/work/repo2"),
        PathBuf::from("/work/repo3"),
    ]
}

#[test]
fn follow_anchor_without_saved_selects_repo_containing_anchor() {
    // Fresh tab, no prior manual pick: follow the anchor to the repo it's in.
    let repos = repos();
    let selected = pick_selected_repo(&repos, Path::new("/work/repo1/src"), true, None, None);
    assert_eq!(selected, Some(0));
}

#[test]
fn follow_anchor_restores_saved_selection_over_anchor_repo() {
    // The reported bug: the anchor lives in repo1, but the user had manually
    // picked repo3 for this tab. Following the anchor must NOT snap back to
    // repo1 — the saved choice wins.
    let repos = repos();
    let saved = PathBuf::from("/work/repo3");
    let selected = pick_selected_repo(
        &repos,
        Path::new("/work/repo1/src"),
        true,
        None,
        Some(&saved),
    );
    assert_eq!(selected, Some(2));
}

#[test]
fn follow_anchor_falls_back_to_anchor_when_saved_repo_gone() {
    // The saved repo no longer exists in the discovered list (e.g. deleted):
    // fall through to following the anchor.
    let repos = repos();
    let saved = PathBuf::from("/work/removed");
    let selected = pick_selected_repo(
        &repos,
        Path::new("/work/repo2/deep/dir"),
        true,
        None,
        Some(&saved),
    );
    assert_eq!(selected, Some(1));
}

#[test]
fn follow_anchor_keeps_previous_when_anchor_outside_all_repos() {
    // Anchor isn't inside any discovered repo and nothing is saved: keep the
    // previously selected repo rather than jumping to the first.
    let repos = repos();
    let previous = PathBuf::from("/work/repo3");
    let selected = pick_selected_repo(&repos, Path::new("/elsewhere"), true, Some(&previous), None);
    assert_eq!(selected, Some(2));
}

#[test]
fn follow_anchor_falls_back_to_first_when_nothing_matches() {
    let repos = repos();
    let selected = pick_selected_repo(&repos, Path::new("/elsewhere"), true, None, None);
    assert_eq!(selected, Some(0));
}

#[test]
fn no_follow_keeps_previous_and_ignores_saved_and_anchor() {
    // Refresh / scan-depth change: keep the current repo. The saved selection
    // and the anchor's own repo are both irrelevant here.
    let repos = repos();
    let previous = PathBuf::from("/work/repo2");
    let saved = PathBuf::from("/work/repo3");
    let selected = pick_selected_repo(
        &repos,
        Path::new("/work/repo1/src"),
        false,
        Some(&previous),
        Some(&saved),
    );
    assert_eq!(selected, Some(1));
}

#[test]
fn no_follow_falls_back_to_first_when_previous_gone() {
    let repos = repos();
    let previous = PathBuf::from("/work/removed");
    let selected = pick_selected_repo(
        &repos,
        Path::new("/work/repo1"),
        false,
        Some(&previous),
        None,
    );
    assert_eq!(selected, Some(0));
}

#[test]
fn empty_repo_list_selects_nothing() {
    let repos: Vec<PathBuf> = Vec::new();
    assert_eq!(
        pick_selected_repo(&repos, Path::new("/work/repo1"), true, None, None),
        None
    );
    assert_eq!(
        pick_selected_repo(&repos, Path::new("/work/repo1"), false, None, None),
        None
    );
}

/// Builds a local-branch [`BranchRef`] from a display name (its full ref is
/// `refs/heads/<name>`); enough for the summary tests, which only read
/// `display_name` / `ref_name`.
fn local_branch(name: &str) -> BranchRef {
    BranchRef {
        display_name: name.to_string(),
        ref_name: format!("refs/heads/{name}"),
        kind: RefKind::LocalBranch,
    }
}

#[test]
fn summary_empty_selection_is_show_all() {
    let branches = vec![local_branch("main"), local_branch("dev")];
    assert_eq!(branch_summary_text(&branches, &HashSet::new()), "Show All");
}

#[test]
fn summary_single_selection_is_the_branch_name() {
    let branches = vec![local_branch("main"), local_branch("dev")];
    let selected = HashSet::from(["refs/heads/dev".to_string()]);
    assert_eq!(branch_summary_text(&branches, &selected), "dev");
}

#[test]
fn summary_multi_selection_joins_in_list_order() {
    // The selection set is unordered; the summary must follow `branches` order
    // (not insertion order) so the button text is stable.
    let branches = vec![
        local_branch("main"),
        local_branch("dev"),
        local_branch("feature"),
    ];
    let selected = HashSet::from([
        "refs/heads/feature".to_string(),
        "refs/heads/main".to_string(),
    ]);
    assert_eq!(branch_summary_text(&branches, &selected), "main, feature");
}

/// The commit details' new elements in the parents they're laid out in; a layout panic fails.
#[test]
fn detail_elements_lay_out() {
    super::super::test_support::lay_out(|appearance| {
        let commit = CommitNode {
            hash: "0123456789abcdef".into(),
            short_hash: "01234567".into(),
            parents: vec!["aaaaaaaaaaaa".into(), "bbbbbbbbbbbb".into()],
            author_name: "Ada Lovelace".into(),
            author_email: "ada@example.com".into(),
            author_time: 0,
            subject: "Merge".into(),
            body_preview: String::new(),
            refs: Vec::new(),
        };
        let states: Vec<MouseStateHandle> = (0..DETAIL_HASH_CHIPS)
            .map(|_| MouseStateHandle::default())
            .collect();
        Flex::column()
            .with_child(with_avatar(
                &commit.author_name,
                &commit.author_email,
                22.,
                Empty::new().finish(),
                appearance,
            ))
            .with_child(render_hash_chips(&commit, &states, appearance))
            .with_child(render_change_bar(3, 1, appearance))
            .with_child(render_status_badge(FileStatus::Renamed, appearance))
            .finish()
    });
}

#[test]
fn relative_time_covers_every_range() {
    assert_eq!(relative_time(5), "just now");
    assert_eq!(relative_time(60), "1 minute ago");
    assert_eq!(relative_time(7_200), "2 hours ago");
    assert_eq!(relative_time(3 * 86_400), "3 days ago");
    assert_eq!(relative_time(65 * 86_400), "2 months ago");
    assert_eq!(relative_time(800 * 86_400), "2 years ago");
}

#[test]
fn nested_file_tree_rows_lay_out() {
    super::super::test_support::lay_out(|appearance| {
        Flex::column()
            .with_child(render_file_tree_row(
                Some(true),
                Icon::Folder,
                None,
                "src",
                0,
                None,
                1,
                1,
                MouseStateHandle::default(),
                GitGraphAction::ToggleDir("src".into()),
                appearance,
            ))
            .with_child(render_file_tree_row(
                None,
                Icon::File,
                Some(FileStatus::Added),
                "deep.rs",
                3,
                Some((5, 0)),
                1,
                1,
                MouseStateHandle::default(),
                GitGraphAction::OpenFileDiff(0),
                appearance,
            ))
            .finish()
    });
}
