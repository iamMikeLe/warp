//! Unit tests for the pure helpers in [`super`] (the write-operation layer):
//! [`GitWriteOp::args`], [`GitWriteOp::confirm_message`], and the small path /
//! ref helpers. IO (`run_write_op`) is exercised by the integration test, not
//! here.

use super::*;

#[test]
fn add_tag_lightweight_omits_annotation_flags() {
    let op = GitWriteOp::AddTag {
        hash: "abc123".into(),
        name: "v1.0".into(),
        message: None,
    };
    assert_eq!(op.args(), vec!["tag", "--end-of-options", "v1.0", "abc123"]);
}

#[test]
fn add_tag_annotated_uses_dash_a_and_message() {
    let op = GitWriteOp::AddTag {
        hash: "abc123".into(),
        name: "v1.0".into(),
        message: Some("first release".into()),
    };
    assert_eq!(
        op.args(),
        vec![
            "tag",
            "-a",
            "-m",
            "first release",
            "--end-of-options",
            "v1.0",
            "abc123"
        ]
    );
}

#[test]
fn create_branch_passes_name_then_hash() {
    let op = GitWriteOp::CreateBranch {
        hash: "deadbeef".into(),
        name: "feature/x".into(),
    };
    assert_eq!(
        op.args(),
        vec!["branch", "--end-of-options", "feature/x", "deadbeef"]
    );
}

#[test]
fn revert_is_no_edit() {
    let op = GitWriteOp::Revert { hash: "h".into() };
    assert_eq!(op.args(), vec!["revert", "--no-edit", "h"]);
}

#[test]
fn drop_commit_rebases_onto_parent() {
    let op = GitWriteOp::DropCommit { hash: "h".into() };
    assert_eq!(op.args(), vec!["rebase", "--onto", "h^", "h"]);
}

#[test]
fn reset_modes_map_to_flags() {
    let cases = [
        (ResetMode::Soft, "--soft"),
        (ResetMode::Mixed, "--mixed"),
        (ResetMode::Hard, "--hard"),
    ];
    for (mode, flag) in cases {
        let op = GitWriteOp::Reset {
            hash: "h".into(),
            mode,
        };
        assert_eq!(op.args(), vec!["reset", flag, "h"]);
    }
}

#[test]
fn delete_remote_branch_uses_push_delete() {
    let op = GitWriteOp::DeleteRemoteBranch {
        remote: "origin".into(),
        branch: "feature".into(),
    };
    assert_eq!(
        op.args(),
        vec!["push", "origin", "--delete", "refs/heads/feature"]
    );
}

#[test]
fn pull_passes_remote_and_branch_separately() {
    let op = GitWriteOp::Pull {
        remote: "origin".into(),
        branch: "main".into(),
    };
    assert_eq!(op.args(), vec!["pull", "origin", "refs/heads/main"]);
}

#[test]
fn rename_branch_uses_branch_dash_m() {
    let op = GitWriteOp::RenameBranch {
        old: "old".into(),
        new: "new".into(),
    };
    assert_eq!(
        op.args(),
        vec!["branch", "-m", "--end-of-options", "old", "new"]
    );
}

#[test]
fn delete_local_branch_uses_branch_dash_d() {
    let op = GitWriteOp::DeleteLocalBranch {
        name: "feature".into(),
        force: false,
    };
    assert_eq!(
        op.args(),
        vec!["branch", "-d", "--end-of-options", "feature"]
    );
}

#[test]
fn delete_local_branch_force_uses_capital_d() {
    let op = GitWriteOp::DeleteLocalBranch {
        name: "feature".into(),
        force: true,
    };
    assert_eq!(
        op.args(),
        vec!["branch", "-D", "--end-of-options", "feature"]
    );
}

#[test]
fn checkout_commit_force_inserts_force_flag() {
    let unforced = GitWriteOp::CheckoutCommit {
        hash: "h".into(),
        force: false,
    };
    assert_eq!(unforced.args(), vec!["checkout", "h"]);
    let forced = GitWriteOp::CheckoutCommit {
        hash: "h".into(),
        force: true,
    };
    assert_eq!(forced.args(), vec!["checkout", "--force", "h"]);
}

#[test]
fn checkout_branch_force_inserts_force_flag() {
    let unforced = GitWriteOp::CheckoutBranch {
        branch: "feat".into(),
        force: false,
    };
    assert_eq!(unforced.args(), vec!["checkout", "feat"]);
    let forced = GitWriteOp::CheckoutBranch {
        branch: "feat".into(),
        force: true,
    };
    assert_eq!(forced.args(), vec!["checkout", "--force", "feat"]);
}

#[test]
fn push_branch_force_uses_force_with_lease() {
    let unforced = GitWriteOp::PushBranch {
        remote: "origin".into(),
        branch: "main".into(),
        force: false,
        set_upstream: false,
    };
    assert_eq!(unforced.args(), vec!["push", "origin", "refs/heads/main"]);
    // Branch force uses --force-with-lease so a moved remote is not clobbered.
    let forced = GitWriteOp::PushBranch {
        remote: "origin".into(),
        branch: "main".into(),
        force: true,
        set_upstream: false,
    };
    assert_eq!(
        forced.args(),
        vec!["push", "--force-with-lease", "origin", "refs/heads/main"]
    );
}

#[test]
fn push_tag_force_uses_plain_force() {
    let unforced = GitWriteOp::PushTag {
        remote: "origin".into(),
        name: "v1".into(),
        force: false,
    };
    assert_eq!(unforced.args(), vec!["push", "origin", "refs/tags/v1"]);
    // Tags have no remote-tracking ref to lease against, so force is bare --force.
    let forced = GitWriteOp::PushTag {
        remote: "origin".into(),
        name: "v1".into(),
        force: true,
    };
    assert_eq!(
        forced.args(),
        vec!["push", "--force", "origin", "refs/tags/v1"]
    );
}

#[test]
fn option_state_is_some_only_for_checkbox_capable_ops() {
    assert_eq!(
        GitWriteOp::PushBranch {
            remote: "o".into(),
            branch: "b".into(),
            force: true,
            set_upstream: false,
        }
        .option_state(),
        Some(true)
    );
    assert_eq!(
        GitWriteOp::CheckoutBranch {
            branch: "b".into(),
            force: false,
        }
        .option_state(),
        Some(false)
    );
    // Clean exposes its "directories" toggle through the same checkbox.
    assert_eq!(
        GitWriteOp::CleanUntracked { directories: true }.option_state(),
        Some(true)
    );
    // An op with no optional flag shows no checkbox.
    assert_eq!(GitWriteOp::Merge { rev: "r".into() }.option_state(), None);
}

#[test]
fn with_option_sets_flag_and_is_noop_for_unsupported() {
    let forced = GitWriteOp::DeleteLocalBranch {
        name: "b".into(),
        force: false,
    }
    .with_option(true);
    assert_eq!(forced.option_state(), Some(true));
    // Clean's directories toggle flows through the same setter.
    let cleaned = GitWriteOp::CleanUntracked { directories: true }.with_option(false);
    assert_eq!(cleaned.option_state(), Some(false));
    // No optional flag → still none after with_option.
    let merge = GitWriteOp::Merge { rev: "r".into() }.with_option(true);
    assert_eq!(merge.option_state(), None);
}

#[test]
fn stash_builds_args_from_message_and_untracked() {
    let plain = GitWriteOp::Stash {
        message: None,
        include_untracked: false,
    };
    assert_eq!(plain.args(), vec!["stash", "push"]);
    let untracked_only = GitWriteOp::Stash {
        message: None,
        include_untracked: true,
    };
    assert_eq!(
        untracked_only.args(),
        vec!["stash", "push", "--include-untracked"]
    );
    // Message comes before the untracked flag.
    let full = GitWriteOp::Stash {
        message: Some("wip".into()),
        include_untracked: true,
    };
    assert_eq!(
        full.args(),
        vec!["stash", "push", "-m", "wip", "--include-untracked"]
    );
}

#[test]
fn stash_ops_build_expected_args() {
    assert_eq!(
        GitWriteOp::StashApply {
            selector: "stash@{0}".into()
        }
        .args(),
        vec!["stash", "apply", "stash@{0}"]
    );
    assert_eq!(
        GitWriteOp::StashPop {
            selector: "stash@{1}".into()
        }
        .args(),
        vec!["stash", "pop", "stash@{1}"]
    );
    assert_eq!(
        GitWriteOp::StashDrop {
            selector: "stash@{0}".into()
        }
        .args(),
        vec!["stash", "drop", "stash@{0}"]
    );
    // branch name precedes the selector.
    assert_eq!(
        GitWriteOp::StashBranch {
            selector: "stash@{0}".into(),
            name: "feature".into(),
        }
        .args(),
        vec![
            "stash",
            "branch",
            "--end-of-options",
            "feature",
            "stash@{0}"
        ]
    );
}

#[test]
fn stash_ops_confirm_except_branch() {
    assert!(
        GitWriteOp::StashApply {
            selector: "stash@{0}".into()
        }
        .confirm_message()
        .is_some()
    );
    // Pop warns it removes the stash.
    assert!(
        GitWriteOp::StashPop {
            selector: "stash@{0}".into()
        }
        .confirm_message()
        .unwrap()
        .contains("removed")
    );
    // Drop warns it cannot be undone.
    assert!(
        GitWriteOp::StashDrop {
            selector: "stash@{0}".into()
        }
        .confirm_message()
        .unwrap()
        .contains("cannot be undone")
    );
    // Create-branch gates through its name dialog, so no confirm.
    assert_eq!(
        GitWriteOp::StashBranch {
            selector: "stash@{0}".into(),
            name: "x".into(),
        }
        .confirm_message(),
        None
    );
}

#[test]
fn stash_gates_through_its_dialog_not_confirm() {
    // The stash dialog gates the op, so it shows no confirm message and no
    // Confirm-dialog checkbox of its own.
    let op = GitWriteOp::Stash {
        message: None,
        include_untracked: true,
    };
    assert_eq!(op.confirm_message(), None);
    assert_eq!(op.option_state(), None);
}

#[test]
fn clean_untracked_dir_toggle_adds_d_flag() {
    let files_only = GitWriteOp::CleanUntracked { directories: false };
    assert_eq!(files_only.args(), vec!["clean", "-f"]);
    let with_dirs = GitWriteOp::CleanUntracked { directories: true };
    assert_eq!(with_dirs.args(), vec!["clean", "-fd"]);
}

#[test]
fn clean_untracked_confirms_irreversible() {
    let msg = GitWriteOp::CleanUntracked { directories: true }
        .confirm_message()
        .expect("clean must confirm");
    assert!(msg.contains("cannot be undone"));
    assert_eq!(
        GitWriteOp::CleanUntracked { directories: true }.option_label(),
        "Clean untracked directories"
    );
}

#[test]
fn rebase_onto_branch_passes_branch_name() {
    let op = GitWriteOp::RebaseOntoBranch {
        branch: "main".into(),
    };
    assert_eq!(op.args(), vec!["rebase", "--end-of-options", "main"]);
}

#[test]
fn delete_tag_uses_dash_d() {
    let op = GitWriteOp::DeleteTag { name: "v1".into() };
    assert_eq!(op.args(), vec!["tag", "-d", "--end-of-options", "v1"]);
}

#[test]
fn archive_includes_format_and_output() {
    let op = GitWriteOp::Archive {
        rev: "main".into(),
        output: PathBuf::from("/tmp/out.zip"),
        format: ArchiveFormat::Zip,
    };
    assert_eq!(
        op.args(),
        vec![
            "archive",
            "--format",
            "zip",
            "-o",
            "/tmp/out.zip",
            "--end-of-options",
            "main"
        ]
    );
}

#[test]
fn archive_format_inferred_from_extension() {
    assert_eq!(
        archive_format_from_path(std::path::Path::new("/tmp/a.zip")),
        ArchiveFormat::Zip
    );
    assert_eq!(
        archive_format_from_path(std::path::Path::new("/tmp/a.tar.gz")),
        ArchiveFormat::TarGz
    );
    assert_eq!(
        archive_format_from_path(std::path::Path::new("/tmp/a.tgz")),
        ArchiveFormat::TarGz
    );
    // Unknown extension falls back to tar.gz.
    assert_eq!(
        archive_format_from_path(std::path::Path::new("/tmp/archive")),
        ArchiveFormat::TarGz
    );
}

#[test]
fn split_remote_ref_splits_on_first_slash() {
    assert_eq!(
        split_remote_ref("origin/feature"),
        ("origin".into(), "feature".into())
    );
    assert_eq!(
        split_remote_ref("origin/feat/x"),
        ("origin".into(), "feat/x".into())
    );
    assert_eq!(split_remote_ref("local"), (String::new(), "local".into()));
}

#[test]
fn confirm_message_present_for_destructive_absent_for_input_ops() {
    // Text-input ops gate themselves through their dialog.
    assert!(
        GitWriteOp::CreateBranch {
            hash: "h".into(),
            name: "b".into()
        }
        .confirm_message()
        .is_none()
    );
    // Hard reset warns about losing uncommitted work.
    let msg = GitWriteOp::Reset {
        hash: "abcdef0".into(),
        mode: ResetMode::Hard,
    }
    .confirm_message()
    .expect("hard reset must confirm");
    assert!(msg.contains("lost"));
    // Remote deletion warns it cannot be undone.
    assert!(
        GitWriteOp::DeleteRemoteBranch {
            remote: "origin".into(),
            branch: "x".into()
        }
        .confirm_message()
        .unwrap()
        .contains("cannot be undone")
    );
}

/// A ref named like an option (valid in git, and fetchable from a remote) must
/// never reach git as a positional argument: `--receive-pack=` / `--upload-pack=`
/// make `git push` / `git pull` run an arbitrary command.
#[test]
fn validate_rejects_option_like_operands() {
    let hostile = "--receive-pack=touch${IFS}pwned;git-receive-pack";
    let ops = [
        GitWriteOp::PushTag {
            remote: "origin".into(),
            name: hostile.into(),
            force: false,
        },
        GitWriteOp::Pull {
            remote: "origin".into(),
            branch: hostile.into(),
        },
        GitWriteOp::DeleteRemoteBranch {
            remote: "origin".into(),
            branch: hostile.into(),
        },
        GitWriteOp::PushBranch {
            remote: hostile.into(),
            branch: "main".into(),
            force: false,
            set_upstream: false,
        },
        GitWriteOp::CheckoutBranch {
            branch: hostile.into(),
            force: false,
        },
        GitWriteOp::DeleteTag {
            name: hostile.into(),
        },
        GitWriteOp::Archive {
            rev: hostile.into(),
            output: PathBuf::from("/tmp/out.zip"),
            format: ArchiveFormat::Zip,
        },
        GitWriteOp::Merge {
            rev: hostile.into(),
        },
        GitWriteOp::RebaseOntoBranch {
            branch: hostile.into(),
        },
        GitWriteOp::DeleteLocalBranch {
            name: hostile.into(),
            force: false,
        },
        GitWriteOp::RenameBranch {
            old: "ok".into(),
            new: hostile.into(),
        },
        GitWriteOp::AddTag {
            hash: "abc123".into(),
            name: hostile.into(),
            message: None,
        },
        GitWriteOp::CreateBranch {
            hash: "abc123".into(),
            name: hostile.into(),
        },
        GitWriteOp::StashBranch {
            selector: "stash@{0}".into(),
            name: hostile.into(),
        },
        GitWriteOp::StashDrop {
            selector: hostile.into(),
        },
        GitWriteOp::Reset {
            hash: hostile.into(),
            mode: ResetMode::Hard,
        },
    ];
    for op in ops {
        assert!(op.validate().is_err(), "should reject: {op:?}");
    }
}

#[test]
fn validate_rejects_empty_operands() {
    let op = GitWriteOp::DeleteTag {
        name: String::new(),
    };
    assert!(op.validate().is_err());
}

#[test]
fn validate_accepts_ordinary_operands_and_option_values() {
    let ops = [
        GitWriteOp::PushTag {
            remote: "origin".into(),
            name: "v1.0".into(),
            force: true,
        },
        GitWriteOp::Pull {
            remote: "origin".into(),
            branch: "feat/x".into(),
        },
        // A message is an option value (`-m <msg>`), so a leading dash is fine.
        GitWriteOp::AddTag {
            hash: "abc123".into(),
            name: "v2".into(),
            message: Some("- bullet notes".into()),
        },
        GitWriteOp::Stash {
            message: Some("--wip".into()),
            include_untracked: true,
        },
        GitWriteOp::CleanUntracked { directories: true },
    ];
    for op in ops {
        assert_eq!(op.validate(), Ok(()), "should accept: {op:?}");
    }
}

/// Remote-sourced names reach `git push` / `git pull` fully qualified, so even
/// without `validate` they can't be parsed as options (`--end-of-options` does
/// not protect `git pull`, which forwards its refspec to `fetch` unguarded).
#[test]
fn remote_sourced_refs_are_fully_qualified() {
    let pull = GitWriteOp::Pull {
        remote: "origin".into(),
        branch: "-x".into(),
    };
    assert_eq!(
        pull.args().last().map(String::as_str),
        Some("refs/heads/-x")
    );
    let push_tag = GitWriteOp::PushTag {
        remote: "origin".into(),
        name: "-x".into(),
        force: false,
    };
    assert_eq!(
        push_tag.args().last().map(String::as_str),
        Some("refs/tags/-x")
    );
}

#[test]
fn staging_passes_paths_literally_after_double_dash() {
    let op = GitWriteOp::StageFiles {
        paths: vec!["--force".into(), ":(glob)*".into()],
    };
    assert_eq!(
        op.args(),
        [
            "--literal-pathspecs",
            "add",
            "-A",
            "--",
            "--force",
            ":(glob)*"
        ]
    );
    assert!(op.validate().is_ok());
    let op = GitWriteOp::UnstageFiles {
        paths: vec!["src/a.rs".into()],
    };
    assert_eq!(
        op.args(),
        ["--literal-pathspecs", "reset", "-q", "--", "src/a.rs"]
    );
    assert!(GitWriteOp::StageFiles { paths: vec![] }.validate().is_err());
    assert!(op.is_staging());
}

#[test]
fn commit_sends_summary_and_optional_description_as_messages() {
    let op = GitWriteOp::Commit {
        summary: "feat: add staging".into(),
        description: "Longer text\n\nwith paragraphs".into(),
    };
    assert_eq!(
        op.args(),
        [
            "commit",
            "-m",
            "feat: add staging",
            "-m",
            "Longer text\n\nwith paragraphs"
        ]
    );
    let op = GitWriteOp::Commit {
        summary: "fix".into(),
        description: "  ".into(),
    };
    assert_eq!(op.args(), ["commit", "-m", "fix"]);
    let empty = GitWriteOp::Commit {
        summary: " ".into(),
        description: String::new(),
    };
    assert!(empty.validate().is_err());
    assert_eq!(empty.confirm_message(), None);
    assert!(!empty.is_staging());
}

#[test]
fn discard_file_picks_the_command_for_each_side() {
    let discard = |side| GitWriteOp::DiscardFile {
        path: "--force".into(),
        side,
    };
    assert_eq!(
        discard(DiscardSide::WorkingTree).args(),
        [
            "--literal-pathspecs",
            "restore",
            "--worktree",
            "--",
            "--force"
        ]
    );
    assert_eq!(
        discard(DiscardSide::Untracked).args(),
        ["--literal-pathspecs", "clean", "-f", "-q", "--", "--force"]
    );
    assert_eq!(
        discard(DiscardSide::StagedAndWorkingTree).args(),
        [
            "--literal-pathspecs",
            "restore",
            "--source=HEAD",
            "--staged",
            "--worktree",
            "--",
            "--force"
        ]
    );
    let op = discard(DiscardSide::Untracked);
    assert!(op.validate().is_ok());
    assert!(op.confirm_message().is_some());
    assert!(op.is_staging());
    assert!(
        GitWriteOp::DiscardFile {
            path: String::new(),
            side: DiscardSide::WorkingTree,
        }
        .validate()
        .is_err()
    );
}

#[test]
fn discard_side_follows_the_entry() {
    let file = |status, staged| ChangedFile {
        path: "a.rs".into(),
        additions: 0,
        deletions: 0,
        status,
        staged,
    };
    assert_eq!(
        DiscardSide::for_file(&file(FileStatus::Added, false)),
        DiscardSide::Untracked
    );
    assert_eq!(
        DiscardSide::for_file(&file(FileStatus::Modified, false)),
        DiscardSide::WorkingTree
    );
    assert_eq!(
        DiscardSide::for_file(&file(FileStatus::Deleted, false)),
        DiscardSide::WorkingTree
    );
    assert_eq!(
        DiscardSide::for_file(&file(FileStatus::Added, true)),
        DiscardSide::StagedAndWorkingTree
    );
}

#[test]
fn push_branch_set_upstream_adds_u_before_the_remote() {
    let op = GitWriteOp::PushBranch {
        remote: "origin".into(),
        branch: "feat".into(),
        force: false,
        set_upstream: true,
    };
    assert_eq!(op.args(), vec!["push", "-u", "origin", "refs/heads/feat"]);
    assert!(op.confirm_message().unwrap().contains("track it"));
}
