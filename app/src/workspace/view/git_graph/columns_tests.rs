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
