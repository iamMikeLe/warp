# Warp + Git Graph

A personal build of [Warp](https://github.com/warpdotdev/warp) with a **Git Graph** panel: a
GitKraken-style commit history view inside the terminal, so browsing history, branches, tags,
and stashes doesn't mean switching to another app. I use it daily alongside AI coding agents,
where many branches, worktrees, and stashes pile up quickly.

> This is an unofficial fork, not affiliated with Warp. It tracks upstream `master` and is
> rebuilt from source.

## Features

- **Commit graph:** a lane-based DAG of the repository's history in resizable
  **Branch / Tag | Graph | Commit message** columns (widths are remembered). Each node shows
  the author's avatar: initials in a per-author color, or their Gravatar picture if you opt in.
  Hovering a "+N" label lists the commit's other branches and tags.
- **Commit details:** message, author, committer, timestamps, and changed files with
  `+/-` counts; clicking a file opens a read-only diff, and clicking a parent hash jumps to it
  (loading older history if needed).
- **Stage and commit:** selecting the WIP row shows unstaged and staged files separately; stage
  or unstage one file (hover it) or everything at once, open either side's diff, and commit with a
  summary (with a 72-character guide) and an optional description. Cmd/Ctrl+Enter commits.
- **Navigation:** a repository picker for folders with several repos, a branch filter,
  paging with "load more", and auto-refresh when the repository changes.
- **Git operations (right-click):** checkout, create/rename/delete branches and tags,
  merge, rebase, reset (soft/mixed/hard), cherry-pick, revert, drop, push/pull, stash
  apply/pop/drop, and archive. Every mutating action asks for confirmation first.
- **Settings → Git:** show or hide the panel and stashes, opt in to Gravatar pictures, and set
  how deep to scan for repositories. The toggles are also in the Command Palette.

## Worktrees panel

A second tools-panel tab (the flowchart icon) for agent-heavy workflows where several worktrees
branch off the same base:

- **Flowchart:** every worktree of the current repository hangs under the branch it was created
  from (read from the branch's reflog, with a fallback for `HEAD`-based worktrees), so a
  `develop` checkout shows the worktrees made from it, nested further if one was made from
  another.
- **Agents:** each card counts the running Claude Code sessions inside that worktree and shows
  each one's state (working, needs you, idle), read from Claude Code's local session files.
  The card's accent and node turn green while an agent works and amber when one needs you.
- **Git state:** branch, commits ahead of/behind its base, uncommitted changes, and the last
  commit.
- **Actions:** click a card to open a terminal tab in it; right-click to copy its path or branch,
  remove it (with confirmation; git refuses dirty worktrees), or prune stale entries.
- Agent states refresh every few seconds and git state about every 30 seconds, only while the
  tab is open. Toggle the tab in Settings → Git or the Command Palette.

## Credits and what I've added

The Git Graph panel was originally written by [@0xBB2B](https://github.com/0xBB2B)
([warpdotdev/warp#12043](https://github.com/warpdotdev/warp/pull/12043), maintained at
[0xBB2B/warp](https://github.com/0xBB2B/warp)). The upstream PR was closed without review, so
I picked it up from there. My work on top of it:

- **Brought it up to date with upstream Warp:** merged several months of upstream changes and
  adapted the feature to Warp's reorganised settings pages and new repository-watcher API.
- **Fixed a missing toolbar button:** the Git Graph tab silently disappeared when all five
  tool panels were enabled.
- **Hardened git argument handling:** the git operations now pass branch and tag names as
  fully qualified refs, use `--end-of-options`, and validate names before running git.
- **Fixed unreadable graphs on repositories with many stashes:** stashes now attach to their
  base commit and respect the branch filter instead of opening lanes that never close (one
  real-world repo needed 168 lanes before the fix), and the lane area is capped so commit
  text always stays visible. Added a "Show stashes" setting.
- **Rebuilt the commit list as a GitKraken-style column layout:** branch/tag labels in their own
  column with a connector to the commit, author avatars as graph nodes, per-lane row tints, and a
  redesigned commit-details panel (hash and parent chips, file-type icons, a folder tree with
  indent guides).

## Branches

- **`git-graph`** (this one): the Git Graph only, kept ready to propose upstream.
- **`worktrees`**: a Worktrees panel built on top of the Git Graph (worktrees under the branches
  they came from, with the coding agents working in each).
- **`ui-tweaks`**: improvements unrelated to git, based directly on upstream Warp: alternating
  terminal block backgrounds, easier pane resizing, colored file icons, and rainbow indent.
- **`pane-collapse`**: collapse a split pane to a thin strip (or bar) without closing it, based
  directly on upstream Warp.
- **`daily`**: everything combined; the build I use day to day.

Rules for working on this fork (for me and for any coding agent working in it):

- **Each feature gets its own branch.** Git Graph work goes on `git-graph`; anything that isn't
  the Git Graph goes on its own branch: stacked on `git-graph` if it needs it (like `worktrees`),
  otherwise based on upstream (like `ui-tweaks`). Then merge it into `daily`. Never commit
  non-Git-Graph changes to `git-graph`, so it stays ready for an upstream PR.
- **Build and run from `daily`.** Code that needs two features together (e.g. colored folder
  icons in the Git Graph details) goes on `daily` only.
- **Merge, don't rebase,** branches that are already pushed. A rewrite needs a force push and
  should be a deliberate decision.

## Building and running (macOS)

Prerequisites: Xcode, Rust via `rustup` (the toolchain version is pinned in
`rust-toolchain.toml`), and:

```sh
brew install protobuf pkgconf
cargo install --locked cargo-bundle@0.11.0
```

Build and launch:

```sh
WARP_SKIP_COMMON_SKILLS_INSTALL=1 ./script/run --features git_graph,open_directory_in_external_editor
```

This opens **WarpOss**, which keeps its own settings in `~/.warp-oss` and doesn't touch an
installed Warp. Open the Git Graph from the **Tools panel**, using the tool icon at the top
left (or the left panel button when vertical tabs are off), then the branch icon.

## Staying up to date with upstream

Merge `upstream/master` into the branches based on upstream, then pass the result down the
stack:

```sh
git fetch upstream
git switch git-graph && git merge upstream/master   # merge, don't rebase: the branch carries the original fork's merge history
git switch ui-tweaks && git merge upstream/master
git switch worktrees && git merge git-graph
git switch daily     && git merge worktrees ui-tweaks
```

## License

Same as Warp: AGPL-3.0 (with MIT-licensed parts); see [LICENSE-AGPL](LICENSE-AGPL) and
[LICENSE-MIT](LICENSE-MIT).

## Trademarks

GitKraken and Warp are trademarks of their respective owners. This project is not affiliated
with or endorsed by either; GitKraken is mentioned only to describe the style of the view.
