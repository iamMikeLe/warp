//! Worktree data: `git worktree list` parsing, the branch each worktree was created from, and
//! the Claude Code sessions running inside each one.
//!
//! Base detection: git doesn't store where a branch came from, but the branch's reflog starts
//! with `branch: Created from <ref>`. When that says `HEAD` (the usual case for agent-created
//! worktrees) or the reflog expired, the base is the first candidate branch that contains the
//! branch's starting commit: the main worktree's branch, then branches checked out in other
//! worktrees, then `main` / `master` / `develop`.
//!
//! Agents: every live Claude Code session writes `~/.claude/sessions/<pid>.json` with its working
//! directory and status. A session counts toward the worktree with the longest path containing
//! its working directory (so an agent in `.claude/worktrees/x` isn't also counted for the main
//! worktree that contains that folder).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// One entry of `git worktree list --porcelain`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorktreeEntry {
    pub path: PathBuf,
    pub head: String,
    /// Short branch name, `None` when detached.
    pub branch: Option<String>,
    pub locked: bool,
    pub prunable: bool,
    /// The repository's main working tree (always listed first by git).
    pub is_main: bool,
}

impl WorktreeEntry {
    /// The folder name, used as the worktree's display name.
    pub(crate) fn name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.to_string_lossy().into_owned())
    }

    /// Whether this looks like a worktree made by an agent tool (`.claude/worktrees/…`,
    /// `.worktrees/…`, or `worktrees/…` inside the repo).
    pub(crate) fn is_agent_worktree(&self) -> bool {
        let path = self.path.to_string_lossy();
        ["/.claude/worktrees/", "/.worktrees/", "/worktrees/"]
            .iter()
            .any(|marker| path.contains(marker))
    }
}

pub(crate) fn parse_worktree_list(stdout: &str) -> Vec<WorktreeEntry> {
    let mut entries = Vec::new();
    for block in stdout.split("\n\n") {
        let mut entry: Option<WorktreeEntry> = None;
        for line in block.lines() {
            let (key, value) = line.split_once(' ').unwrap_or((line, ""));
            match key {
                "worktree" => {
                    entry = Some(WorktreeEntry {
                        path: PathBuf::from(value),
                        head: String::new(),
                        branch: None,
                        locked: false,
                        prunable: false,
                        is_main: entries.is_empty(),
                    });
                }
                "HEAD" => {
                    if let Some(e) = entry.as_mut() {
                        e.head = value.to_string();
                    }
                }
                "branch" => {
                    if let Some(e) = entry.as_mut() {
                        e.branch = Some(value.strip_prefix("refs/heads/").unwrap_or(value).into());
                    }
                }
                "locked" => {
                    if let Some(e) = entry.as_mut() {
                        e.locked = true;
                    }
                }
                "prunable" => {
                    if let Some(e) = entry.as_mut() {
                        e.prunable = true;
                    }
                }
                _ => {}
            }
        }
        entries.extend(entry);
    }
    entries
}

/// What a branch's oldest reflog entry says it was created from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BranchOrigin {
    /// The commit the branch started at.
    pub start: String,
    /// The ref named in `Created from <ref>`; `None` for `HEAD` or other messages.
    pub from: Option<String>,
}

/// Parses `git reflog show --format=%H%x1f%gs <branch>` output (newest first); the last line is
/// the branch's creation.
pub(crate) fn parse_branch_origin(stdout: &str) -> Option<BranchOrigin> {
    let line = stdout.lines().rfind(|l| !l.trim().is_empty())?;
    let (start, message) = line.split_once('\u{1f}')?;
    let from = message
        .strip_prefix("branch: Created from ")
        .map(str::trim)
        .filter(|r| !r.is_empty() && *r != "HEAD" && !r.starts_with("refs/stash"))
        .map(|r| r.strip_prefix("refs/heads/").unwrap_or(r).to_string());
    Some(BranchOrigin {
        start: start.trim().to_string(),
        from,
    })
}

/// A Claude Code agent's state, from its session file's `status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AgentStatus {
    Working,
    /// Waiting for the user (a permission prompt or a question).
    Waiting,
    Idle,
    Other(String),
}

impl AgentStatus {
    fn parse(status: &str) -> Self {
        let lower = status.to_ascii_lowercase();
        match lower.as_str() {
            "busy" | "working" | "running" | "thinking" => Self::Working,
            "idle" | "ready" | "done" => Self::Idle,
            _ if ["wait", "permission", "input", "blocked", "ask"]
                .iter()
                .any(|w| lower.contains(w)) =>
            {
                Self::Waiting
            }
            _ => Self::Other(status.to_string()),
        }
    }

    pub(crate) fn label(&self) -> &str {
        match self {
            Self::Working => "working",
            Self::Waiting => "needs you",
            Self::Idle => "idle",
            Self::Other(status) => status,
        }
    }

    /// For summaries: the most urgent status first.
    fn urgency(&self) -> u8 {
        match self {
            Self::Waiting => 0,
            Self::Working => 1,
            Self::Other(_) => 2,
            Self::Idle => 3,
        }
    }
}

/// A live Claude Code session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentSession {
    pub pid: u32,
    pub name: String,
    pub cwd: PathBuf,
    pub status: AgentStatus,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionFile {
    pid: u32,
    cwd: PathBuf,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

/// Parses one `~/.claude/sessions/<pid>.json`. Only the fields the view shows are read.
pub(crate) fn parse_session(json: &str) -> Option<AgentSession> {
    let file: SessionFile = serde_json::from_str(json).ok()?;
    Some(AgentSession {
        pid: file.pid,
        name: file.name.unwrap_or_default(),
        cwd: file.cwd,
        status: AgentStatus::parse(file.status.as_deref().unwrap_or("idle")),
    })
}

/// The most urgent status among `agents` (waiting > working > other > idle).
pub(crate) fn summary_status(agents: &[AgentSession]) -> Option<&AgentStatus> {
    agents.iter().map(|a| &a.status).min_by_key(|s| s.urgency())
}

/// For each worktree path, the sessions whose working directory is inside it, matching each
/// session to the most specific (longest) worktree path.
pub(crate) fn assign_agents(
    worktrees: &[PathBuf],
    sessions: Vec<AgentSession>,
) -> Vec<Vec<AgentSession>> {
    let mut assigned = vec![Vec::new(); worktrees.len()];
    for session in sessions {
        let best = worktrees
            .iter()
            .enumerate()
            .filter(|(_, path)| session.cwd.starts_with(path))
            .max_by_key(|(_, path)| path.as_os_str().len());
        if let Some((index, _)) = best {
            assigned[index].push(session);
        }
    }
    for agents in &mut assigned {
        agents.sort_by(|a, b| {
            a.status
                .urgency()
                .cmp(&b.status.urgency())
                .then(a.pid.cmp(&b.pid))
        });
    }
    assigned
}

/// Picks the base branch of `branch`: the reflog's explicit `Created from` ref when it's a
/// known branch, else the first candidate that contains the starting commit.
/// `contains(candidate)` reports whether `candidate` contains `origin.start`.
pub(crate) fn choose_base(
    branch: &str,
    origin: Option<&BranchOrigin>,
    candidates: &[String],
    known_branches: &[String],
    mut contains: impl FnMut(&str) -> bool,
) -> Option<String> {
    if let Some(from) = origin.and_then(|o| o.from.as_deref())
        && let Some(local) = local_branch_for(from, known_branches)
        && local != branch
    {
        return Some(local.to_string());
    }
    candidates
        .iter()
        .filter(|c| c.as_str() != branch)
        .find(|c| origin.is_none() || contains(c))
        .cloned()
}

/// `name` if it's a local branch, or the local branch a remote-tracking name like
/// `origin/develop` refers to.
fn local_branch_for<'a>(name: &str, known_branches: &'a [String]) -> Option<&'a str> {
    let known = |n: &str| known_branches.iter().find(|b| *b == n).map(String::as_str);
    known(name).or_else(|| name.split_once('/').and_then(|(_, rest)| known(rest)))
}

/// Everything shown on a worktree card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorktreeInfo {
    pub entry: WorktreeEntry,
    pub base: Option<String>,
    pub changed_files: usize,
    /// Commits on the worktree's branch not on its base, and the reverse.
    pub ahead: usize,
    pub behind: usize,
    pub last_subject: String,
    /// Unix seconds.
    pub last_time: i64,
    pub agents: Vec<AgentSession>,
}

/// Parses `git rev-list --left-right --count base...branch` (`behind\tahead`).
pub(crate) fn parse_ahead_behind(stdout: &str) -> (usize, usize) {
    let mut parts = stdout.split_whitespace().map(|n| n.parse().unwrap_or(0));
    let behind = parts.next().unwrap_or(0);
    let ahead = parts.next().unwrap_or(0);
    (ahead, behind)
}

/// Reads every live Claude Code session. Missing or unreadable files are skipped.
#[cfg(not(target_family = "wasm"))]
pub(crate) fn load_agent_sessions() -> Vec<AgentSession> {
    let Some(dir) = dirs::home_dir().map(|home| home.join(".claude").join("sessions")) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .filter_map(|json| parse_session(&json))
        .filter(|session| process_alive(session.pid))
        .collect()
}

/// Session files of crashed sessions can linger, so only running processes count.
#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    // Signal 0 checks existence without signalling; EPERM still means the process exists.
    match nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None) {
        Ok(()) => true,
        Err(err) => err == nix::errno::Errno::EPERM,
    }
}

#[cfg(all(not(unix), not(target_family = "wasm")))]
fn process_alive(_pid: u32) -> bool {
    true
}

#[cfg(not(target_family = "wasm"))]
async fn git(dir: &Path, args: &[&str]) -> anyhow::Result<String> {
    warp_util::git::run_git_command(dir, args).await
}

/// The repository's main working tree for any directory inside it (including inside a linked
/// worktree), so every worktree of the repository is listed wherever the terminal is.
#[cfg(not(target_family = "wasm"))]
pub(crate) async fn main_worktree_for(dir: &Path) -> Option<PathBuf> {
    let stdout = git(dir, &["worktree", "list", "--porcelain"]).await.ok()?;
    parse_worktree_list(&stdout)
        .into_iter()
        .next()
        .map(|entry| entry.path)
}

/// Loads every worktree of the repository at `repo` with its base, status, and agents.
#[cfg(not(target_family = "wasm"))]
pub(crate) async fn load_worktrees(repo: &Path) -> anyhow::Result<Vec<WorktreeInfo>> {
    let entries = parse_worktree_list(&git(repo, &["worktree", "list", "--porcelain"]).await?);
    let known_branches: Vec<String> = git(
        repo,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
    )
    .await
    .map(|out| out.lines().map(str::to_string).collect())
    .unwrap_or_default();

    // Candidate bases, in order of preference.
    let mut candidates: Vec<String> = Vec::new();
    for branch in entries
        .iter()
        .filter_map(|e| e.branch.clone())
        .chain(["main", "master", "develop"].map(String::from))
    {
        if known_branches.contains(&branch) && !candidates.contains(&branch) {
            candidates.push(branch);
        }
    }

    let paths: Vec<PathBuf> = entries.iter().map(|e| e.path.clone()).collect();
    let mut agents = assign_agents(&paths, load_agent_sessions()).into_iter();

    let mut infos = Vec::with_capacity(entries.len());
    let mut contains_cache: HashMap<(String, String), bool> = HashMap::new();
    for entry in entries {
        let agents = agents.next().unwrap_or_default();
        let mut info = WorktreeInfo {
            base: None,
            changed_files: 0,
            ahead: 0,
            behind: 0,
            last_subject: String::new(),
            last_time: 0,
            agents,
            entry,
        };
        if info.entry.prunable {
            infos.push(info);
            continue;
        }
        let path = info.entry.path.clone();
        info.changed_files = git(&path, &["status", "--porcelain"])
            .await
            .map(|out| out.lines().filter(|l| !l.trim().is_empty()).count())
            .unwrap_or(0);
        if let Ok(out) = git(&path, &["log", "-1", "--format=%ct%x1f%s", "HEAD"]).await
            && let Some((time, subject)) = out.trim_end().split_once('\u{1f}')
        {
            info.last_time = time.parse().unwrap_or(0);
            info.last_subject = subject.to_string();
        }
        if let Some(branch) = info.entry.branch.clone() {
            let full = format!("refs/heads/{branch}");
            let origin = git(repo, &["reflog", "show", "--format=%H%x1f%gs", &full])
                .await
                .ok()
                .and_then(|out| parse_branch_origin(&out));
            // Candidates are checked one by one, so `contains` can't await; resolve them first.
            let needs_candidates = origin.as_ref().is_some_and(|o| {
                o.from
                    .as_deref()
                    .and_then(|from| local_branch_for(from, &known_branches))
                    .is_none()
            });
            if let Some(origin) = origin.as_ref().filter(|_| needs_candidates) {
                for candidate in candidates.iter().filter(|c| **c != branch) {
                    let key = (candidate.clone(), origin.start.clone());
                    if contains_cache.contains_key(&key) {
                        continue;
                    }
                    let candidate_ref = format!("refs/heads/{candidate}");
                    let contained = git(
                        repo,
                        &[
                            "merge-base",
                            "--is-ancestor",
                            "--end-of-options",
                            &origin.start,
                            &candidate_ref,
                        ],
                    )
                    .await
                    .is_ok();
                    contains_cache.insert(key, contained);
                    if contained {
                        break;
                    }
                }
            }
            info.base = choose_base(
                &branch,
                origin.as_ref(),
                &candidates,
                &known_branches,
                |candidate| {
                    origin.as_ref().is_some_and(|o| {
                        contains_cache
                            .get(&(candidate.to_string(), o.start.clone()))
                            .copied()
                            .unwrap_or(false)
                    })
                },
            );
            if let Some(base) = &info.base {
                let range = format!("refs/heads/{base}...{full}");
                if let Ok(out) = git(
                    repo,
                    &[
                        "rev-list",
                        "--left-right",
                        "--count",
                        "--end-of-options",
                        &range,
                    ],
                )
                .await
                {
                    (info.ahead, info.behind) = parse_ahead_behind(&out);
                }
            }
        }
        infos.push(info);
    }
    Ok(infos)
}

/// Removes a linked worktree (refuses a dirty one, as plain `git worktree remove` does).
#[cfg(not(target_family = "wasm"))]
pub(crate) async fn remove_worktree(repo: &Path, path: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(path.is_absolute(), "worktree path must be absolute");
    let path = path.to_string_lossy().into_owned();
    git(repo, &["worktree", "remove", "--end-of-options", &path]).await?;
    Ok(())
}

/// Cleans up records of worktrees whose folders were deleted.
#[cfg(not(target_family = "wasm"))]
pub(crate) async fn prune_worktrees(repo: &Path) -> anyhow::Result<()> {
    git(repo, &["worktree", "prune"]).await?;
    Ok(())
}

#[cfg(test)]
#[path = "data_tests.rs"]
mod tests;
