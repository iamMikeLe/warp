use super::*;

const LIST: &str = "worktree /repo\nHEAD aaa\nbranch refs/heads/develop\n\n\
worktree /repo/.claude/worktrees/fix-login\nHEAD bbb\nbranch refs/heads/fix-login\n\n\
worktree /elsewhere/spike\nHEAD ccc\ndetached\nlocked reason\n\n\
worktree /gone\nHEAD ddd\nbranch refs/heads/old\nprunable gitdir file points to non-existent location\n";

#[test]
fn parses_worktree_list() {
    let entries = parse_worktree_list(LIST);
    assert_eq!(entries.len(), 4);
    assert!(entries[0].is_main);
    assert_eq!(entries[0].branch.as_deref(), Some("develop"));
    assert!(!entries[1].is_main);
    assert!(entries[1].is_agent_worktree());
    assert_eq!(entries[1].name(), "fix-login");
    assert_eq!(entries[2].branch, None);
    assert!(entries[2].locked);
    assert!(entries[3].prunable);
    assert!(!entries[0].is_agent_worktree());
}

#[test]
fn branch_origin_reads_the_oldest_reflog_entry() {
    let reflog = "c2\u{1f}commit: more\nc1\u{1f}branch: Created from develop\n";
    assert_eq!(
        parse_branch_origin(reflog),
        Some(BranchOrigin {
            start: "c1".into(),
            from: Some("develop".into())
        })
    );
    let from_head = "c1\u{1f}branch: Created from HEAD\n";
    assert_eq!(parse_branch_origin(from_head).unwrap().from, None);
    assert_eq!(parse_branch_origin(""), None);
}

fn branches(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| n.to_string()).collect()
}

#[test]
fn explicit_origin_wins_and_remote_names_map_to_local_branches() {
    let known = branches(&["develop", "main", "feat"]);
    let origin = BranchOrigin {
        start: "c1".into(),
        from: Some("origin/develop".into()),
    };
    let base = choose_base("feat", Some(&origin), &branches(&["main"]), &known, |_| {
        true
    });
    assert_eq!(base.as_deref(), Some("develop"));
}

#[test]
fn created_from_head_picks_the_first_candidate_containing_the_start() {
    let known = branches(&["develop", "main", "feat"]);
    let origin = BranchOrigin {
        start: "c1".into(),
        from: None,
    };
    let candidates = branches(&["feat", "main", "develop"]);
    let base = choose_base("feat", Some(&origin), &candidates, &known, |c| {
        c == "develop"
    });
    assert_eq!(base.as_deref(), Some("develop"));
    let none = choose_base("feat", Some(&origin), &candidates, &known, |_| false);
    assert_eq!(none, None);
}

#[test]
fn without_a_reflog_the_first_other_candidate_is_used() {
    let known = branches(&["develop", "feat"]);
    let base = choose_base(
        "feat",
        None,
        &branches(&["feat", "develop"]),
        &known,
        |_| false,
    );
    assert_eq!(base.as_deref(), Some("develop"));
}

#[test]
fn parses_sessions_and_statuses() {
    let json = r#"{"pid": 42, "sessionId": "s", "cwd": "/repo/.claude/worktrees/x",
        "name": "agent-1", "status": "busy", "updatedAt": 1}"#;
    let session = parse_session(json).unwrap();
    assert_eq!(session.pid, 42);
    assert_eq!(session.status, AgentStatus::Working);
    assert_eq!(session.name, "agent-1");
    assert_eq!(AgentStatus::parse("idle"), AgentStatus::Idle);
    assert_eq!(
        AgentStatus::parse("waiting_for_permission"),
        AgentStatus::Waiting
    );
    assert_eq!(
        AgentStatus::parse("compacting"),
        AgentStatus::Other("compacting".into())
    );
    assert_eq!(parse_session("not json"), None);
}

fn session(pid: u32, cwd: &str, status: AgentStatus) -> AgentSession {
    AgentSession {
        pid,
        name: String::new(),
        cwd: cwd.into(),
        status,
    }
}

#[test]
fn agents_go_to_the_most_specific_worktree() {
    let worktrees = vec![
        PathBuf::from("/repo"),
        PathBuf::from("/repo/.claude/worktrees/x"),
    ];
    let assigned = assign_agents(
        &worktrees,
        vec![
            session(1, "/repo/.claude/worktrees/x/src", AgentStatus::Idle),
            session(2, "/repo", AgentStatus::Working),
            session(3, "/repo/.claude/worktrees/x", AgentStatus::Waiting),
            session(4, "/other", AgentStatus::Working),
            // A sibling folder sharing the prefix is not inside the worktree.
            session(5, "/repo-copy", AgentStatus::Working),
        ],
    );
    assert_eq!(assigned[0].iter().map(|s| s.pid).collect::<Vec<_>>(), [2]);
    assert_eq!(
        assigned[1].iter().map(|s| s.pid).collect::<Vec<_>>(),
        [3, 1]
    );
    assert_eq!(summary_status(&assigned[1]), Some(&AgentStatus::Waiting));
    assert_eq!(summary_status(&[]), None);
}

#[test]
fn parses_ahead_behind() {
    assert_eq!(parse_ahead_behind("3\t5\n"), (5, 3));
    assert_eq!(parse_ahead_behind(""), (0, 0));
}
