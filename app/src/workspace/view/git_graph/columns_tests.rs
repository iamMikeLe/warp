//! Unit tests for the pure helpers in [`super`] (column layout).

use super::*;

fn label(kind: RefKind, name: &str) -> RefLabel {
    RefLabel {
        kind,
        name: name.into(),
    }
}

#[test]
fn no_refs_means_no_primary() {
    assert_eq!(primary_ref(&[]), None);
}

#[test]
fn current_branch_wins_over_other_refs() {
    let refs = [
        label(RefKind::Tag, "v1.0"),
        label(RefKind::RemoteBranch, "origin/dev"),
        label(RefKind::Head, "main"),
        label(RefKind::LocalBranch, "dev"),
    ];
    let primary = primary_ref(&refs).unwrap();
    assert_eq!(primary.label.name, "main");
    assert_eq!(primary.hidden, 3);
}

#[test]
fn local_branch_preferred_then_remote_then_tag_then_stash() {
    let refs = [
        label(RefKind::Stash, "stash@{0}"),
        label(RefKind::Tag, "v2"),
        label(RefKind::RemoteBranch, "origin/x"),
    ];
    assert_eq!(primary_ref(&refs).unwrap().label.name, "origin/x");
    let refs = [
        label(RefKind::Stash, "stash@{0}"),
        label(RefKind::Tag, "v2"),
    ];
    assert_eq!(primary_ref(&refs).unwrap().label.name, "v2");
}

#[test]
fn same_named_remote_folds_into_the_local_label() {
    let refs = [
        label(RefKind::LocalBranch, "feat/x"),
        label(RefKind::RemoteBranch, "origin/feat/x"),
        label(RefKind::Tag, "v1"),
    ];
    let primary = primary_ref(&refs).unwrap();
    assert_eq!(primary.label.name, "feat/x");
    assert_eq!(
        primary.tracked_remote.map(|r| r.name.as_str()),
        Some("origin/feat/x")
    );
    // Only the tag is left over.
    assert_eq!(primary.hidden, 1);
}

#[test]
fn differently_named_remote_is_not_folded() {
    let refs = [
        label(RefKind::LocalBranch, "feat/x"),
        label(RefKind::RemoteBranch, "origin/feat/y"),
    ];
    let primary = primary_ref(&refs).unwrap();
    assert_eq!(primary.tracked_remote, None);
    assert_eq!(primary.hidden, 1);
}

#[test]
fn remote_primary_does_not_fold_anything() {
    let refs = [
        label(RefKind::RemoteBranch, "origin/main"),
        label(RefKind::RemoteBranch, "upstream/main"),
    ];
    let primary = primary_ref(&refs).unwrap();
    assert_eq!(primary.label.name, "origin/main");
    assert_eq!(primary.tracked_remote, None);
    assert_eq!(primary.hidden, 1);
}

/// Regression: the commit details lay ref pills out in a `Wrap`, which gives each child an
/// unbounded width; the pill's shrinkable name must not need a bounded parent.
#[test]
fn ref_pill_lays_out_in_an_unbounded_wrap() {
    super::super::test_support::lay_out(|appearance| {
        let label = RefLabel {
            kind: RefKind::LocalBranch,
            name: "feature/a-very-long-branch-name-that-needs-an-ellipsis".into(),
        };
        warpui::elements::Wrap::row()
            .with_children([ref_pill(&label, true, lane_color(1), appearance)])
            .finish()
    });
}

#[test]
fn commit_row_lays_out_with_refs_and_body() {
    super::super::test_support::lay_out(|appearance| {
        let commit = CommitNode {
            hash: "abc".into(),
            short_hash: "abc".into(),
            parents: vec!["def".into()],
            author_name: "Ada Lovelace".into(),
            author_email: "ada@example.com".into(),
            author_time: 0,
            subject: "feat: a subject".into(),
            body_preview: "a body".into(),
            refs: vec![
                RefLabel {
                    kind: RefKind::Head,
                    name: "main".into(),
                },
                RefLabel {
                    kind: RefKind::RemoteBranch,
                    name: "origin/main".into(),
                },
                RefLabel {
                    kind: RefKind::Tag,
                    name: "v1".into(),
                },
            ],
        };
        let layout = super::super::layout::assign_lanes(std::slice::from_ref(&commit));
        let widths = ColumnWidths {
            refs: DEFAULT_REFS_WIDTH,
            graph: DEFAULT_GRAPH_WIDTH,
        };
        render_commit_row(
            &layout.rows[0],
            layout.max_lanes,
            &commit,
            0,
            "pos",
            widths,
            MouseStateHandle::default(),
            false,
            appearance,
        )
    });
}

#[test]
fn hidden_refs_skip_the_shown_label_and_its_remote() {
    let refs = vec![
        RefLabel {
            kind: RefKind::Tag,
            name: "v1".into(),
        },
        RefLabel {
            kind: RefKind::LocalBranch,
            name: "main".into(),
        },
        RefLabel {
            kind: RefKind::RemoteBranch,
            name: "origin/main".into(),
        },
        RefLabel {
            kind: RefKind::RemoteBranch,
            name: "upstream/dev".into(),
        },
    ];
    let primary = primary_ref(&refs).unwrap();
    let hidden: Vec<&str> = primary
        .hidden_refs(&refs)
        .iter()
        .map(|r| r.name.as_str())
        .collect();
    assert_eq!(hidden, ["v1", "upstream/dev"]);
    assert_eq!(hidden.len(), primary.hidden);
}
