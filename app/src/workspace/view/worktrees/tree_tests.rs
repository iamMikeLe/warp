use std::path::PathBuf;

use super::super::data::{WorktreeEntry, WorktreeInfo};
use super::*;

fn worktree(path: &str, branch: Option<&str>, base: Option<&str>, is_main: bool) -> WorktreeInfo {
    WorktreeInfo {
        entry: WorktreeEntry {
            path: PathBuf::from(path),
            head: String::new(),
            branch: branch.map(String::from),
            locked: false,
            prunable: false,
            is_main,
        },
        base: base.map(String::from),
        changed_files: 0,
        ahead: 0,
        behind: 0,
        last_subject: String::new(),
        last_time: 0,
        agents: Vec::new(),
    }
}

fn shape(rows: &[TreeRow]) -> Vec<(NodeKind, usize, bool)> {
    rows.iter()
        .map(|r| (r.kind.clone(), r.depth, r.is_last))
        .collect()
}

#[test]
fn worktrees_hang_under_the_worktree_holding_their_base() {
    // The main checkout is on develop (made from main, which no worktree has checked out, so it
    // becomes a branch node); three worktrees were made from develop and one from fix-a.
    let worktrees = vec![
        worktree("/repo", Some("develop"), Some("main"), true),
        worktree(
            "/repo/.claude/worktrees/c",
            Some("c"),
            Some("develop"),
            false,
        ),
        worktree(
            "/repo/.claude/worktrees/a",
            Some("fix-a"),
            Some("develop"),
            false,
        ),
        worktree(
            "/repo/.claude/worktrees/b",
            Some("b"),
            Some("develop"),
            false,
        ),
        worktree("/wt/a2", Some("a2"), Some("fix-a"), false),
    ];
    let rows = build_tree(&worktrees);
    assert_eq!(
        shape(&rows),
        [
            (NodeKind::Branch("main".into()), 0, true),
            (NodeKind::Worktree(0), 1, true),
            (NodeKind::Worktree(2), 2, false),
            (NodeKind::Worktree(4), 3, true),
            (NodeKind::Worktree(3), 2, false),
            (NodeKind::Worktree(1), 2, true),
        ]
    );
    // The a2 row: the column for "fix-a"'s siblings (b, c) continues past it.
    assert_eq!(rows[3].ancestors_continue, [false, true]);
    assert!(rows[2].has_children);
}

#[test]
fn main_worktree_without_base_is_the_first_root() {
    let worktrees = vec![
        worktree("/repo", Some("develop"), None, true),
        worktree("/x", Some("x"), Some("develop"), false),
        worktree("/y", None, None, false),
    ];
    let rows = build_tree(&worktrees);
    assert_eq!(
        shape(&rows),
        [
            (NodeKind::Worktree(0), 0, false),
            (NodeKind::Worktree(1), 1, true),
            (NodeKind::Worktree(2), 0, true),
        ]
    );
}

#[test]
fn cycles_are_broken() {
    let worktrees = vec![
        worktree("/a", Some("a"), Some("b"), true),
        worktree("/b", Some("b"), Some("a"), false),
    ];
    let rows = build_tree(&worktrees);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows.iter().filter(|r| r.depth == 0).count(), 1);
}
