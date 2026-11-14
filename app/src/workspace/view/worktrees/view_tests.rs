use std::path::PathBuf;

use warpui::elements::{ClippedScrollStateHandle, MouseStateHandle};

use super::super::data::{AgentSession, AgentStatus, WorktreeEntry, WorktreeInfo};
use super::super::tree::build_tree;
use super::render_tree;
use crate::workspace::view::git_graph::test_support::lay_out;

fn worktree(
    path: &str,
    branch: &str,
    base: Option<&str>,
    agents: Vec<AgentSession>,
) -> WorktreeInfo {
    WorktreeInfo {
        entry: WorktreeEntry {
            path: PathBuf::from(path),
            head: "0123456789abcdef".into(),
            branch: Some(branch.into()),
            locked: false,
            prunable: false,
            is_main: base.is_none(),
        },
        base: base.map(String::from),
        changed_files: 3,
        ahead: 2,
        behind: 1,
        last_subject: "feat: a long commit subject that should be clipped with an ellipsis".into(),
        last_time: 0,
        agents,
    }
}

fn agent(pid: u32, status: AgentStatus) -> AgentSession {
    AgentSession {
        pid,
        name: format!("agent-{pid}"),
        cwd: PathBuf::from("/repo"),
        status,
    }
}

/// The whole flowchart (branch node, nested cards, agent rows with overflow) lays out without a
/// flex panic.
#[test]
fn flowchart_lays_out() {
    lay_out(|appearance| {
        let worktrees = vec![
            worktree("/repo", "develop", None, vec![agent(1, AgentStatus::Idle)]),
            worktree(
                "/repo/.claude/worktrees/feature-with-a-very-long-folder-name",
                "feature/login",
                Some("develop"),
                (2..8).map(|pid| agent(pid, AgentStatus::Working)).collect(),
            ),
            worktree(
                "/wt/b",
                "b",
                Some("feature/login"),
                vec![agent(9, AgentStatus::Waiting)],
            ),
            worktree("/wt/c", "c", Some("release"), Vec::new()),
        ];
        let rows = build_tree(&worktrees);
        let states: Vec<MouseStateHandle> =
            (0..worktrees.len()).map(|_| Default::default()).collect();
        render_tree(
            &rows,
            &worktrees,
            &states,
            "pos",
            ClippedScrollStateHandle::new(),
            appearance,
        )
    });
}
