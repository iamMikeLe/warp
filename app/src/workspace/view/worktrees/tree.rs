//! Arranges worktrees into the flowchart: each worktree hangs under the branch it was created
//! from. A base branch checked out in another worktree is that worktree's card (so a worktree made
//! from another worktree's branch nests under it); a base branch with no worktree gets its own
//! branch node. Rows come out in depth-first order with the guide lines each row draws.

use std::collections::{BTreeMap, HashSet};

use super::data::WorktreeInfo;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NodeKind {
    /// A base branch not checked out in any worktree.
    Branch(String),
    /// Index into the worktree list.
    Worktree(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TreeRow {
    pub kind: NodeKind,
    pub depth: usize,
    /// For each ancestor column `0..depth - 1`: whether a guide line continues through this row
    /// (the ancestor at that level has siblings below).
    pub ancestors_continue: Vec<bool>,
    /// Whether this is its parent's last child (the elbow stops at the node).
    pub is_last: bool,
    pub has_children: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Branch(String),
    Worktree(usize),
}

pub(crate) fn build_tree(worktrees: &[WorktreeInfo]) -> Vec<TreeRow> {
    let by_branch: BTreeMap<&str, usize> = worktrees
        .iter()
        .enumerate()
        .filter_map(|(i, w)| w.entry.branch.as_deref().map(|b| (b, i)))
        .collect();

    let parent_of = |i: usize| -> Option<Key> {
        let base = worktrees[i].base.as_deref()?;
        match by_branch.get(base) {
            Some(&j) if j != i => Some(Key::Worktree(j)),
            Some(_) => None,
            None => Some(Key::Branch(base.to_string())),
        }
    };

    // Break cycles (a's base is b's branch and b's base is a's): walk up from each worktree and
    // detach it from its parent if the walk returns to it.
    let mut parents: Vec<Option<Key>> = (0..worktrees.len()).map(parent_of).collect();
    for i in 0..worktrees.len() {
        let mut seen = HashSet::from([i]);
        let mut current = parents[i].clone();
        while let Some(Key::Worktree(j)) = current {
            if !seen.insert(j) {
                parents[i] = None;
                break;
            }
            current = parents[j].clone();
        }
    }

    let mut children: BTreeMap<Key, Vec<usize>> = BTreeMap::new();
    let mut roots: Vec<Key> = Vec::new();
    for (i, parent) in parents.iter().enumerate() {
        match parent {
            Some(key) => {
                children.entry(key.clone()).or_default().push(i);
                if let Key::Branch(_) = key
                    && !roots.contains(key)
                {
                    roots.push(key.clone());
                }
            }
            None => roots.push(Key::Worktree(i)),
        }
    }
    // The main worktree first, then the other root worktrees, then bare branch nodes by name.
    roots.sort_by_key(|key| match key {
        Key::Worktree(i) => (u8::from(!worktrees[*i].entry.is_main), *i, String::new()),
        Key::Branch(name) => (2, 0, name.clone()),
    });
    for list in children.values_mut() {
        list.sort_by_key(|&i| (worktrees[i].entry.name().to_lowercase(), i));
    }

    let mut rows = Vec::new();
    let count = roots.len();
    for (n, root) in roots.into_iter().enumerate() {
        push_rows(&root, 0, &[], n + 1 == count, &children, &mut rows);
    }
    rows
}

fn push_rows(
    key: &Key,
    depth: usize,
    ancestors_continue: &[bool],
    is_last: bool,
    children: &BTreeMap<Key, Vec<usize>>,
    rows: &mut Vec<TreeRow>,
) {
    let kids = children.get(key).map(Vec::as_slice).unwrap_or_default();
    rows.push(TreeRow {
        kind: match key {
            Key::Branch(name) => NodeKind::Branch(name.clone()),
            Key::Worktree(i) => NodeKind::Worktree(*i),
        },
        depth,
        ancestors_continue: ancestors_continue.to_vec(),
        is_last,
        has_children: !kids.is_empty(),
    });
    // Roots are separate trees: no guide runs between them.
    let mut below = ancestors_continue.to_vec();
    if depth > 0 {
        below.push(!is_last);
    }
    for (n, &child) in kids.iter().enumerate() {
        push_rows(
            &Key::Worktree(child),
            depth + 1,
            &below,
            n + 1 == kids.len(),
            children,
            rows,
        );
    }
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
