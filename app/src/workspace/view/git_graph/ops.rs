//! Write-operation layer for the Git Graph: the set of mutating git actions
//! reachable from the right-click context menus (checkout, branch/tag
//! create/delete, merge, rebase, reset, cherry-pick, revert, push/pull,
//! archive).
//!
//! Mirrors [`super::data`]'s split of pure logic from IO: [`GitWriteOp::args`]
//! and [`GitWriteOp::confirm_message`] are pure (unit-tested in `ops_tests.rs`),
//! while [`run_write_op`] is a thin async wrapper over
//! [`warp_util::git::run_git_command`]. Every mutating action is gated at the UI
//! layer by [`warp_features::FeatureFlag::GitGraphWrite`].

#[cfg(not(target_family = "wasm"))]
use std::path::Path;
use std::path::PathBuf;

#[cfg(not(target_family = "wasm"))]
use anyhow::Result;

use super::data::{ChangedFile, FileStatus};

/// `git reset` mode (chosen from the "Reset current branch to this Commit"
/// submenu). Each moves the current branch ref to the target commit; they
/// differ in what happens to the index and working tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResetMode {
    /// Keep index and working tree (only move the branch ref).
    Soft,
    /// Reset the index, keep the working tree (git's default).
    Mixed,
    /// Reset index and working tree — discards uncommitted changes.
    Hard,
}

impl ResetMode {
    fn flag(self) -> &'static str {
        match self {
            ResetMode::Soft => "--soft",
            ResetMode::Mixed => "--mixed",
            ResetMode::Hard => "--hard",
        }
    }
}

/// Output format for "Create Archive", derived from the file extension the user
/// picks in the save dialog. `git archive` accepts both `zip` and `tar.gz`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArchiveFormat {
    Zip,
    TarGz,
}

impl ArchiveFormat {
    fn name(self) -> &'static str {
        match self {
            ArchiveFormat::Zip => "zip",
            ArchiveFormat::TarGz => "tar.gz",
        }
    }
}

/// Infers the archive format from the chosen output path's extension; defaults
/// to [`ArchiveFormat::TarGz`] when the extension is neither `.zip` nor a
/// gzip-tar variant (`.tar.gz` / `.tgz`).
pub(crate) fn archive_format_from_path(path: &std::path::Path) -> ArchiveFormat {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if name.ends_with(".zip") {
        ArchiveFormat::Zip
    } else {
        ArchiveFormat::TarGz
    }
}

/// Splits a remote-branch display name (`origin/feature`, `origin/feat/x`) into
/// its remote and branch parts on the first `/`. A name without a `/` is
/// treated as having no remote prefix (the whole string is the branch).
pub(crate) fn split_remote_ref(display_name: &str) -> (String, String) {
    match display_name.split_once('/') {
        Some((remote, branch)) => (remote.to_string(), branch.to_string()),
        None => (String::new(), display_name.to_string()),
    }
}

/// Ends option parsing so a following ref or name is never read as a flag.
/// Supported by parse-options git commands since git 2.24. `git checkout` and
/// `git pull` don't honor it (checkout treats it as the start of pathspecs, pull
/// forwards its refspec to `fetch` unguarded), so those ops rely on fully
/// qualified refs and [`GitWriteOp::validate`] instead.
const END_OF_OPTIONS: &str = "--end-of-options";

/// `refs/heads/<branch>`: a fully qualified branch ref can't start with `-`, so
/// a hostile branch name from a remote (e.g. `--upload-pack=<cmd>`) can't be
/// parsed by `git push` / `git pull` as an option.
/// Makes git read pathspecs as plain paths (no globs or `:(magic)`).
const LITERAL_PATHSPECS: &str = "--literal-pathspecs";

fn qualified_branch(branch: &str) -> String {
    format!("refs/heads/{branch}")
}

/// `refs/tags/<name>`; see [`qualified_branch`].
fn qualified_tag(name: &str) -> String {
    format!("refs/tags/{name}")
}

/// A fully-specified, ready-to-run mutating git operation. The UI builds one of
/// these once it has all the inputs it needs (text from a prompt dialog, a mode
/// from the reset submenu, a path from the save dialog), then runs it through
/// [`run_write_op`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GitWriteOp {
    /// `git tag <name> <hash>` (lightweight) or `git tag -a <name> -m <msg>
    /// <hash>` (annotated, when a message is given).
    AddTag {
        hash: String,
        name: String,
        message: Option<String>,
    },
    /// `git branch <name> <hash>`.
    CreateBranch { hash: String, name: String },
    /// `git checkout [--force] <hash>` (detached HEAD); `force` discards
    /// uncommitted changes that would otherwise block the switch.
    CheckoutCommit { hash: String, force: bool },
    /// `git cherry-pick <hash>` onto the current branch.
    CherryPick { hash: String },
    /// `git revert --no-edit <hash>` on the current branch.
    Revert { hash: String },
    /// Drop a commit from the current branch's history via
    /// `git rebase --onto <hash>^ <hash>` (rewrites history).
    DropCommit { hash: String },
    /// `git merge <rev>` into the current branch (`rev` is a commit hash or a
    /// remote-branch ref like `origin/feature`).
    Merge { rev: String },
    /// `git rebase <hash>` — rebase the current branch onto the commit.
    Rebase { hash: String },
    /// `git rebase <branch>` — rebase the current branch onto another branch.
    RebaseOntoBranch { branch: String },
    /// `git reset --soft|--mixed|--hard <hash>` — move the current branch ref.
    Reset { hash: String, mode: ResetMode },
    /// `git clean -f` (or `git clean -fd` when `directories`) — removes untracked
    /// files from the working tree. `-f` is mandatory (git refuses to clean
    /// without it); `directories` adds `-d` to also remove untracked directories,
    /// surfaced as the confirm dialog's (default-on) checkbox.
    CleanUntracked { directories: bool },
    /// `git stash push` with an optional message (`-m`) and `--include-untracked`
    /// when `include_untracked`. Restorable later with `git stash pop`. Reached
    /// through the stash dialog (message input + untracked checkbox), so it gates
    /// itself there — no confirm message, no Confirm-dialog checkbox of its own.
    Stash {
        message: Option<String>,
        include_untracked: bool,
    },
    /// `git stash apply <selector>` — apply a stash onto the working tree, keeping
    /// the stash in the list. `selector` is `stash@{n}`.
    StashApply { selector: String },
    /// `git stash pop <selector>` — apply a stash and drop it on success.
    StashPop { selector: String },
    /// `git stash drop <selector>` — delete a stash without applying it.
    StashDrop { selector: String },
    /// `git stash branch <name> <selector>` — create a branch at the stash's base
    /// commit, apply the stash onto it, and drop the stash. Reached through a
    /// text-input dialog (the branch name), so it has no confirm message.
    StashBranch { selector: String, name: String },
    /// `git checkout [--force] <branch>` — check out a (remote) branch by its
    /// short name, letting git set up tracking; `force` discards uncommitted
    /// changes that would otherwise block the switch.
    CheckoutBranch { branch: String, force: bool },
    /// `git push <remote> --delete <branch>`.
    DeleteRemoteBranch { remote: String, branch: String },
    /// `git pull <remote> <branch>` into the current branch.
    Pull { remote: String, branch: String },
    /// `git branch -m <old> <new>`.
    RenameBranch { old: String, new: String },
    /// `git branch -d|-D <name>` — delete a local branch; `force` (`-D`) deletes
    /// it even when it isn't fully merged.
    DeleteLocalBranch { name: String, force: bool },
    /// `git push [--force-with-lease] <remote> <branch>`. `force` uses
    /// `--force-with-lease` (not bare `--force`) so the overwrite is refused if
    /// the remote moved past the position we last fetched, guarding others' work.
    PushBranch {
        remote: String,
        branch: String,
        force: bool,
    },
    /// `git tag -d <name>`.
    DeleteTag { name: String },
    /// `git push [--force] <remote> <tag>`. Uses bare `--force` (not
    /// `--force-with-lease`): tags have no remote-tracking ref for a lease to
    /// compare against, so a lease would always be refused.
    PushTag {
        remote: String,
        name: String,
        force: bool,
    },
    /// `git add -A -- <paths>`: stages the files' changes, including deletions.
    StageFiles { paths: Vec<String> },
    /// `git add -A`: stages every change in the working tree.
    StageAll,
    /// `git reset -q -- <paths>`: unstages the files, keeping their changes in the working tree.
    UnstageFiles { paths: Vec<String> },
    /// `git reset -q`: unstages everything.
    UnstageAll,
    /// Throws away one side of a file's uncommitted change (see [`DiscardSide`]).
    DiscardFile { path: String, side: DiscardSide },
    /// `git commit -m <summary> [-m <description>]` of the staged changes.
    Commit {
        summary: String,
        description: String,
    },
    /// `git archive --format <fmt> -o <output> <rev>`.
    Archive {
        rev: String,
        output: PathBuf,
        format: ArchiveFormat,
    },
}

/// Which uncommitted change of a file [`GitWriteOp::DiscardFile`] throws away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiscardSide {
    /// `git restore --worktree`: the working tree's changes to a tracked file, back to the index.
    WorkingTree,
    /// `git clean -f`: deletes an untracked file.
    Untracked,
    /// `git restore --source=HEAD --staged --worktree`: the file's staged *and* unstaged changes,
    /// back to HEAD (a file new since HEAD is removed).
    StagedAndWorkingTree,
}

impl DiscardSide {
    /// The side discarded for an uncommitted-changes entry. An unstaged added file is untracked:
    /// it isn't in the index, so only `git clean` removes it.
    pub(crate) fn for_file(file: &ChangedFile) -> Self {
        match (file.staged, file.status) {
            (true, _) => DiscardSide::StagedAndWorkingTree,
            (false, FileStatus::Added) => DiscardSide::Untracked,
            (false, FileStatus::Modified | FileStatus::Deleted | FileStatus::Renamed) => {
                DiscardSide::WorkingTree
            }
        }
    }
}

impl GitWriteOp {
    /// Quick updates to the uncommitted changes (staging, unstaging, discarding a file), which
    /// run without the working overlay.
    pub(crate) fn is_staging(&self) -> bool {
        matches!(
            self,
            GitWriteOp::StageFiles { .. }
                | GitWriteOp::StageAll
                | GitWriteOp::UnstageFiles { .. }
                | GitWriteOp::UnstageAll
                | GitWriteOp::DiscardFile { .. }
        )
    }

    /// The exact `git` argument vector for this operation. Pure (no IO) so it can
    /// be unit-tested; `run_write_op` borrows these as `&str`s.
    pub(crate) fn args(&self) -> Vec<String> {
        match self {
            GitWriteOp::AddTag {
                hash,
                name,
                message: None,
            } => vec![
                "tag".into(),
                END_OF_OPTIONS.into(),
                name.clone(),
                hash.clone(),
            ],
            GitWriteOp::AddTag {
                hash,
                name,
                message: Some(msg),
            } => vec![
                "tag".into(),
                "-a".into(),
                "-m".into(),
                msg.clone(),
                END_OF_OPTIONS.into(),
                name.clone(),
                hash.clone(),
            ],
            GitWriteOp::CreateBranch { hash, name } => {
                vec![
                    "branch".into(),
                    END_OF_OPTIONS.into(),
                    name.clone(),
                    hash.clone(),
                ]
            }
            GitWriteOp::CheckoutCommit { hash, force } => {
                let mut args = vec!["checkout".into()];
                if *force {
                    args.push("--force".into());
                }
                args.push(hash.clone());
                args
            }
            GitWriteOp::CherryPick { hash } => vec!["cherry-pick".into(), hash.clone()],
            GitWriteOp::Revert { hash } => {
                vec!["revert".into(), "--no-edit".into(), hash.clone()]
            }
            GitWriteOp::DropCommit { hash } => vec![
                "rebase".into(),
                "--onto".into(),
                format!("{hash}^"),
                hash.clone(),
            ],
            GitWriteOp::Merge { rev } => {
                vec!["merge".into(), END_OF_OPTIONS.into(), rev.clone()]
            }
            GitWriteOp::Rebase { hash } => {
                vec!["rebase".into(), END_OF_OPTIONS.into(), hash.clone()]
            }
            GitWriteOp::RebaseOntoBranch { branch } => {
                vec!["rebase".into(), END_OF_OPTIONS.into(), branch.clone()]
            }
            GitWriteOp::Reset { hash, mode } => {
                vec!["reset".into(), mode.flag().into(), hash.clone()]
            }
            GitWriteOp::CleanUntracked { directories } => {
                let flag = if *directories { "-fd" } else { "-f" };
                vec!["clean".into(), flag.into()]
            }
            GitWriteOp::Stash {
                message,
                include_untracked,
            } => {
                let mut args = vec!["stash".into(), "push".into()];
                if let Some(msg) = message {
                    args.push("-m".into());
                    args.push(msg.clone());
                }
                if *include_untracked {
                    args.push("--include-untracked".into());
                }
                args
            }
            GitWriteOp::StashApply { selector } => {
                vec!["stash".into(), "apply".into(), selector.clone()]
            }
            GitWriteOp::StashPop { selector } => {
                vec!["stash".into(), "pop".into(), selector.clone()]
            }
            GitWriteOp::StashDrop { selector } => {
                vec!["stash".into(), "drop".into(), selector.clone()]
            }
            GitWriteOp::StashBranch { selector, name } => vec![
                "stash".into(),
                "branch".into(),
                END_OF_OPTIONS.into(),
                name.clone(),
                selector.clone(),
            ],
            GitWriteOp::CheckoutBranch { branch, force } => {
                let mut args = vec!["checkout".into()];
                if *force {
                    args.push("--force".into());
                }
                args.push(branch.clone());
                args
            }
            GitWriteOp::DeleteRemoteBranch { remote, branch } => vec![
                "push".into(),
                remote.clone(),
                "--delete".into(),
                qualified_branch(branch),
            ],
            GitWriteOp::Pull { remote, branch } => {
                vec!["pull".into(), remote.clone(), qualified_branch(branch)]
            }
            GitWriteOp::RenameBranch { old, new } => {
                vec![
                    "branch".into(),
                    "-m".into(),
                    END_OF_OPTIONS.into(),
                    old.clone(),
                    new.clone(),
                ]
            }
            GitWriteOp::DeleteLocalBranch { name, force } => {
                let flag = if *force { "-D" } else { "-d" };
                vec![
                    "branch".into(),
                    flag.into(),
                    END_OF_OPTIONS.into(),
                    name.clone(),
                ]
            }
            GitWriteOp::PushBranch {
                remote,
                branch,
                force,
            } => {
                let mut args = vec!["push".into()];
                if *force {
                    args.push("--force-with-lease".into());
                }
                args.push(remote.clone());
                args.push(qualified_branch(branch));
                args
            }
            GitWriteOp::DeleteTag { name } => vec![
                "tag".into(),
                "-d".into(),
                END_OF_OPTIONS.into(),
                name.clone(),
            ],
            GitWriteOp::PushTag {
                remote,
                name,
                force,
            } => {
                let mut args = vec!["push".into()];
                if *force {
                    args.push("--force".into());
                }
                args.push(remote.clone());
                args.push(qualified_tag(name));
                args
            }
            // Paths go after `--` and are read literally, so a file named like an option or a
            // pathspec pattern (`:(glob)*`, `*.rs`) only ever means that file.
            GitWriteOp::StageFiles { paths } => {
                let mut args = vec![
                    LITERAL_PATHSPECS.into(),
                    "add".into(),
                    "-A".into(),
                    "--".into(),
                ];
                args.extend(paths.iter().cloned());
                args
            }
            GitWriteOp::StageAll => vec!["add".into(), "-A".into()],
            GitWriteOp::UnstageFiles { paths } => {
                let mut args = vec![
                    LITERAL_PATHSPECS.into(),
                    "reset".into(),
                    "-q".into(),
                    "--".into(),
                ];
                args.extend(paths.iter().cloned());
                args
            }
            GitWriteOp::UnstageAll => vec!["reset".into(), "-q".into()],
            GitWriteOp::DiscardFile { path, side } => {
                let mut args = vec![LITERAL_PATHSPECS.to_string()];
                let command: &[&str] = match side {
                    DiscardSide::WorkingTree => &["restore", "--worktree"],
                    DiscardSide::Untracked => &["clean", "-f", "-q"],
                    DiscardSide::StagedAndWorkingTree => {
                        &["restore", "--source=HEAD", "--staged", "--worktree"]
                    }
                };
                args.extend(command.iter().map(|arg| arg.to_string()));
                args.push("--".into());
                args.push(path.clone());
                args
            }
            GitWriteOp::Commit {
                summary,
                description,
            } => {
                let mut args = vec!["commit".into(), "-m".into(), summary.clone()];
                if !description.trim().is_empty() {
                    args.push("-m".into());
                    args.push(description.clone());
                }
                args
            }
            GitWriteOp::Archive {
                rev,
                output,
                format,
            } => vec![
                "archive".into(),
                "--format".into(),
                format.name().into(),
                "-o".into(),
                output.to_string_lossy().into_owned(),
                END_OF_OPTIONS.into(),
                rev.clone(),
            ],
        }
    }

    /// Every ref, name, remote, hash, or stash selector this op hands git as a
    /// positional argument. Option values (`-m <message>`, `-o <path>`) are
    /// excluded: git always reads those as values.
    fn operands(&self) -> Vec<&str> {
        match self {
            GitWriteOp::AddTag { hash, name, .. } | GitWriteOp::CreateBranch { hash, name } => {
                vec![name, hash]
            }
            GitWriteOp::CheckoutCommit { hash, .. }
            | GitWriteOp::CherryPick { hash }
            | GitWriteOp::Revert { hash }
            | GitWriteOp::DropCommit { hash }
            | GitWriteOp::Rebase { hash }
            | GitWriteOp::Reset { hash, .. } => vec![hash],
            GitWriteOp::Merge { rev } | GitWriteOp::Archive { rev, .. } => vec![rev],
            GitWriteOp::RebaseOntoBranch { branch } | GitWriteOp::CheckoutBranch { branch, .. } => {
                vec![branch]
            }
            GitWriteOp::CleanUntracked { .. }
            | GitWriteOp::Stash { .. }
            | GitWriteOp::StageAll
            | GitWriteOp::UnstageAll
            | GitWriteOp::Commit { .. } => vec![],
            // Paths follow `--`, where a leading `-` can't be read as an option.
            GitWriteOp::StageFiles { .. }
            | GitWriteOp::UnstageFiles { .. }
            | GitWriteOp::DiscardFile { .. } => vec![],
            GitWriteOp::StashApply { selector }
            | GitWriteOp::StashPop { selector }
            | GitWriteOp::StashDrop { selector } => vec![selector],
            GitWriteOp::StashBranch { selector, name } => vec![name, selector],
            GitWriteOp::DeleteRemoteBranch { remote, branch }
            | GitWriteOp::Pull { remote, branch }
            | GitWriteOp::PushBranch { remote, branch, .. } => vec![remote, branch],
            GitWriteOp::RenameBranch { old, new } => vec![old, new],
            GitWriteOp::DeleteLocalBranch { name, .. } | GitWriteOp::DeleteTag { name } => {
                vec![name]
            }
            GitWriteOp::PushTag { remote, name, .. } => vec![remote, name],
        }
    }

    /// Refuses an op whose positional arguments could be parsed as git options.
    /// Refs, tags, and remote branches can arrive from a remote with names like
    /// `--receive-pack=<cmd>` (valid ref names), which several git commands would
    /// otherwise treat as an option that runs a command. This is the backstop for
    /// every op, including those where [`END_OF_OPTIONS`] isn't honored.
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    pub(crate) fn validate(&self) -> std::result::Result<(), String> {
        match self {
            GitWriteOp::Commit { summary, .. } if summary.trim().is_empty() => {
                return Err("A commit needs a summary.".to_string());
            }
            GitWriteOp::StageFiles { paths } | GitWriteOp::UnstageFiles { paths }
                if paths.is_empty() || paths.iter().any(String::is_empty) =>
            {
                return Err("No files to stage or unstage.".to_string());
            }
            GitWriteOp::DiscardFile { path, .. } if path.is_empty() => {
                return Err("No file to discard.".to_string());
            }
            _ => {}
        }
        for operand in self.operands() {
            if operand.is_empty() {
                return Err("Refusing to run git with an empty name.".to_string());
            }
            if operand.starts_with('-') {
                return Err(format!(
                    "Refusing to run git: \"{operand}\" starts with \"-\" and could be read as an option."
                ));
            }
        }
        Ok(())
    }

    /// Names the user typed that git will create, as `(name, is_branch)`; checked
    /// with `git check-ref-format` before running.
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    fn new_ref_names(&self) -> Vec<(&str, bool)> {
        match self {
            GitWriteOp::AddTag { name, .. } => vec![(name, false)],
            GitWriteOp::CreateBranch { name, .. } | GitWriteOp::StashBranch { name, .. } => {
                vec![(name, true)]
            }
            GitWriteOp::RenameBranch { new, .. } => vec![(new, true)],
            _ => vec![],
        }
    }

    /// The confirmation prompt shown before running this operation. Operations
    /// reached through a text-input dialog (add tag / create branch / rename)
    /// return `None` — their dialog already gates them — as does the OS save
    /// dialog for archives; everything else returns a tailored yes/no message so
    /// the user always sees what is about to happen (especially history-rewriting
    /// or remote-mutating actions).
    pub(crate) fn confirm_message(&self) -> Option<String> {
        let short = |h: &str| h.chars().take(8).collect::<String>();
        match self {
            GitWriteOp::AddTag { .. }
            | GitWriteOp::CreateBranch { .. }
            | GitWriteOp::RenameBranch { .. }
            | GitWriteOp::Stash { .. }
            | GitWriteOp::StashBranch { .. }
            | GitWriteOp::StageFiles { .. }
            | GitWriteOp::StageAll
            | GitWriteOp::UnstageFiles { .. }
            | GitWriteOp::UnstageAll
            | GitWriteOp::Commit { .. }
            | GitWriteOp::Archive { .. } => None,
            GitWriteOp::CheckoutCommit { hash, .. } => Some(format!(
                "Check out commit {} as a detached HEAD?",
                short(hash)
            )),
            GitWriteOp::CherryPick { hash } => Some(format!(
                "Cherry-pick commit {} onto the current branch?",
                short(hash)
            )),
            GitWriteOp::Revert { hash } => Some(format!(
                "Revert commit {} on the current branch?",
                short(hash)
            )),
            GitWriteOp::DropCommit { hash } => Some(format!(
                "Drop commit {} from the current branch? This rewrites history.",
                short(hash)
            )),
            GitWriteOp::Merge { rev } => Some(format!("Merge {} into the current branch?", rev)),
            GitWriteOp::Rebase { hash } => Some(format!(
                "Rebase the current branch onto {}? This rewrites history.",
                short(hash)
            )),
            GitWriteOp::RebaseOntoBranch { branch } => Some(format!(
                "Rebase the current branch onto \"{branch}\"? This rewrites history."
            )),
            GitWriteOp::DeleteLocalBranch { name, .. } => {
                Some(format!("Delete branch \"{name}\"? This cannot be undone."))
            }
            GitWriteOp::Reset { hash, mode } => Some(match mode {
                ResetMode::Hard => format!(
                    "Hard-reset the current branch to {}? Uncommitted changes will be lost.",
                    short(hash)
                ),
                ResetMode::Soft => {
                    format!("Soft-reset the current branch to {}?", short(hash))
                }
                ResetMode::Mixed => {
                    format!("Reset the current branch to {}?", short(hash))
                }
            }),
            GitWriteOp::CheckoutBranch { branch, .. } => {
                Some(format!("Check out branch \"{branch}\"?"))
            }
            GitWriteOp::CleanUntracked { .. } => Some(
                "Remove all untracked files from the working tree? This cannot be undone."
                    .to_string(),
            ),
            GitWriteOp::StashApply { selector } => {
                Some(format!("Apply stash \"{selector}\" onto the working tree?"))
            }
            GitWriteOp::StashPop { selector } => Some(format!(
                "Pop stash \"{selector}\"? It is applied to the working tree and then removed."
            )),
            GitWriteOp::StashDrop { selector } => {
                Some(format!("Drop stash \"{selector}\"? This cannot be undone."))
            }
            GitWriteOp::DeleteRemoteBranch { remote, branch } => Some(format!(
                "Delete branch \"{branch}\" from remote \"{remote}\"? This cannot be undone."
            )),
            GitWriteOp::Pull { remote, branch } => {
                Some(format!("Pull {remote}/{branch} into the current branch?"))
            }
            GitWriteOp::PushBranch { remote, branch, .. } => {
                Some(format!("Push branch \"{branch}\" to \"{remote}\"?"))
            }
            GitWriteOp::DeleteTag { name } => {
                Some(format!("Delete tag \"{name}\"? This cannot be undone."))
            }
            GitWriteOp::PushTag { remote, name, .. } => {
                Some(format!("Push tag \"{name}\" to \"{remote}\"?"))
            }
            GitWriteOp::DiscardFile { path, side } => Some(match side {
                DiscardSide::WorkingTree => {
                    format!("Discard unstaged changes to \"{path}\"? This cannot be undone.")
                }
                DiscardSide::Untracked => {
                    format!("Delete untracked file \"{path}\"? This cannot be undone.")
                }
                DiscardSide::StagedAndWorkingTree => format!(
                    "Discard all changes to \"{path}\", staged and unstaged? This cannot be undone."
                ),
            }),
        }
    }

    /// State of the confirmation dialog's optional checkbox for this op: `None`
    /// hides the checkbox, `Some(checked)` shows it pre-set to `checked`. Covers
    /// the force flag on checkout/delete/push and the "clean untracked
    /// directories" toggle on clean.
    pub(crate) fn option_state(&self) -> Option<bool> {
        match self {
            GitWriteOp::CheckoutCommit { force, .. }
            | GitWriteOp::CheckoutBranch { force, .. }
            | GitWriteOp::DeleteLocalBranch { force, .. }
            | GitWriteOp::PushBranch { force, .. }
            | GitWriteOp::PushTag { force, .. } => Some(*force),
            GitWriteOp::CleanUntracked { directories } => Some(*directories),
            _ => None,
        }
    }

    /// Returns this op with its dialog-checkbox flag set to `value`; a no-op for
    /// ops that have no such option.
    pub(crate) fn with_option(mut self, value: bool) -> Self {
        match &mut self {
            GitWriteOp::CheckoutCommit { force, .. }
            | GitWriteOp::CheckoutBranch { force, .. }
            | GitWriteOp::DeleteLocalBranch { force, .. }
            | GitWriteOp::PushBranch { force, .. }
            | GitWriteOp::PushTag { force, .. } => *force = value,
            GitWriteOp::CleanUntracked { directories } => *directories = value,
            _ => {}
        }
        self
    }

    /// Label for the confirmation dialog's checkbox, naming what toggling it does
    /// for this specific op (only ever shown where [`Self::option_state`] is
    /// `Some`).
    pub(crate) fn option_label(&self) -> &'static str {
        match self {
            GitWriteOp::CheckoutCommit { .. } | GitWriteOp::CheckoutBranch { .. } => {
                "Force (discard local changes)"
            }
            GitWriteOp::DeleteLocalBranch { .. } => "Force (delete unmerged branch)",
            GitWriteOp::PushBranch { .. } => "Force (overwrite remote)",
            GitWriteOp::PushTag { .. } => "Force (overwrite remote tag)",
            GitWriteOp::CleanUntracked { .. } => "Clean untracked directories",
            _ => "Force",
        }
    }
}

/// Runs a mutating git operation in `repo_root`. Thin async wrapper over
/// [`warp_util::git::run_git_command`]; the success/failure is surfaced by the
/// caller (a reload on success, an error banner on failure).
#[cfg(not(target_family = "wasm"))]
pub(crate) async fn run_write_op(repo_root: &Path, op: &GitWriteOp) -> Result<()> {
    op.validate().map_err(anyhow::Error::msg)?;
    for (name, is_branch) in op.new_ref_names() {
        // `validate` already rejected a leading `-`, so `name` can't be taken as
        // an option here either.
        let check = if is_branch {
            warp_util::git::run_git_command(repo_root, &["check-ref-format", "--branch", name])
                .await
        } else {
            let full = qualified_tag(name);
            warp_util::git::run_git_command(repo_root, &["check-ref-format", full.as_str()]).await
        };
        if check.is_err() {
            anyhow::bail!("\"{name}\" is not a valid git ref name.");
        }
    }
    let args = op.args();
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    warp_util::git::run_git_command(repo_root, &arg_refs).await?;
    Ok(())
}

#[cfg(test)]
#[path = "ops_tests.rs"]
mod tests;
