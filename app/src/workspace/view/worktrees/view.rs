//! The Worktrees panel: a flowchart of the repository's worktrees under the branches they were
//! created from, with each worktree's git state and the Claude Code agents working in it.
//!
//! Refreshing: agent sessions are re-read every [`AGENT_POLL`] (cheap file reads); the git side
//! reloads every [`GIT_POLL`], on manual refresh, and after a remove / prune. Polling only runs
//! while the panel is the active tools-panel tab.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use pathfinder_color::ColorU;
use warp_core::ui::Icon;
use warp_core::ui::theme::color::internal_colors;
use warpui::clipboard::ClipboardContent;
use warpui::elements::{
    Align, ChildAnchor, ChildView, ClippedScrollStateHandle, ClippedScrollable, ConstrainedBox,
    Container, CornerRadius, CrossAxisAlignment, Dismiss, Element, Empty, Expanded, Fill, Flex,
    Hoverable, MainAxisAlignment, MainAxisSize, MouseStateHandle, OffsetPositioning, ParentElement,
    PositionedElementAnchor, PositionedElementOffsetBounds, Radius, SavePosition, ScrollbarWidth,
    Shrinkable, Stack, Text,
};
use warpui::fonts::{Properties, Weight};
use warpui::geometry::vector::{Vector2F, vec2f};
use warpui::text_layout::ClipConfig;
use warpui::ui_components::components::UiComponent;
use warpui::{AppContext, Entity, SingletonEntity, TypedActionView, View, ViewContext, ViewHandle};

use super::data::{AgentStatus, WorktreeInfo, summary_status};
use super::gutter::TreeGutter;
use super::tree::{NodeKind, TreeRow, build_tree};
use crate::appearance::Appearance;
use crate::menu::{Menu, MenuItem, MenuItemFields};
use crate::ui_components::buttons::icon_button;
use crate::workspace::view::git_graph::view::relative_time;

const AGENT_POLL: Duration = Duration::from_secs(3);
/// Every this many agent polls, the git state reloads too (~30 s).
const GIT_POLL_EVERY: u32 = 10;
const CARD_HEIGHT: f32 = 86.0;
const CARD_GAP: f32 = 8.0;
const BRANCH_ROW_HEIGHT: f32 = 30.0;
/// The node sits level with the card's title line.
const CARD_NODE_Y: f32 = 17.0;
const CONTEXT_MENU_WIDTH: f32 = 220.0;

const WORKING: ColorU = ColorU {
    r: 0x4e,
    g: 0xc9,
    b: 0x7a,
    a: 0xff,
};
const WAITING: ColorU = ColorU {
    r: 0xff,
    g: 0xb0,
    b: 0x4f,
    a: 0xff,
};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum WorktreesAction {
    Refresh,
    OpenInNewTab(PathBuf),
    OpenMenu { index: usize, x: f32, y: f32 },
    CopyText(String),
    PromptRemove(PathBuf),
    ConfirmRemove,
    CancelRemove,
    Prune,
    DismissError,
}

pub(crate) enum WorktreesEvent {
    /// Open a new terminal tab in this directory.
    OpenDirectoryInNewTab(PathBuf),
}

enum LoadState {
    NoRepo,
    Loading,
    Loaded,
    Failed(String),
}

pub(crate) struct WorktreesView {
    /// The terminal's working directory the panel follows.
    anchor: Option<PathBuf>,
    /// The main worktree of the repository containing `anchor`.
    repo: Option<PathBuf>,
    state: LoadState,
    worktrees: Arc<Vec<WorktreeInfo>>,
    rows: Arc<Vec<TreeRow>>,
    card_mouse_states: Vec<MouseStateHandle>,
    refresh_mouse_state: MouseStateHandle,
    confirm_mouse_states: [MouseStateHandle; 2],
    scroll_state: ClippedScrollStateHandle,
    /// Guards against overlapping git loads.
    loading: bool,
    /// Whether the panel is the visible tools-panel tab (polling runs only then).
    active: bool,
    /// Bumped whenever polling restarts, so an older poll loop stops.
    poll_generation: u64,
    polls: u32,
    position_id: String,
    context_menu: ViewHandle<Menu<WorktreesAction>>,
    menu_open: bool,
    menu_offset: Vector2F,
    confirm_remove: Option<PathBuf>,
    error: Option<String>,
}

impl WorktreesView {
    pub(crate) fn new(ctx: &mut ViewContext<Self>) -> Self {
        let context_menu = ctx.add_typed_action_view(|_| {
            Menu::new()
                .with_width(CONTEXT_MENU_WIDTH)
                .prevent_interaction_with_other_elements()
                .with_drop_shadow()
        });
        ctx.subscribe_to_view(&context_menu, |me, _, event, ctx| {
            if let crate::menu::Event::Close { .. } = event {
                me.menu_open = false;
                ctx.notify();
            }
        });
        Self {
            anchor: None,
            repo: None,
            state: LoadState::NoRepo,
            worktrees: Arc::new(Vec::new()),
            rows: Arc::new(Vec::new()),
            card_mouse_states: Vec::new(),
            refresh_mouse_state: MouseStateHandle::default(),
            confirm_mouse_states: Default::default(),
            scroll_state: ClippedScrollStateHandle::new(),
            loading: false,
            active: false,
            poll_generation: 0,
            polls: 0,
            position_id: format!("worktrees_{}", ctx.view_id()),
            context_menu,
            menu_open: false,
            menu_offset: Vector2F::zero(),
            confirm_remove: None,
            error: None,
        }
    }

    /// Follows the focused terminal's directory: the panel shows that directory's repository.
    pub(crate) fn set_working_directory(
        &mut self,
        dir: Option<PathBuf>,
        ctx: &mut ViewContext<Self>,
    ) {
        if self.anchor == dir {
            return;
        }
        self.anchor = dir;
        if self.active {
            self.resolve_repo(ctx);
        }
    }

    /// Called when the tab is shown or hidden; loads on show and polls while shown.
    pub(crate) fn set_active(&mut self, active: bool, ctx: &mut ViewContext<Self>) {
        if self.active == active {
            return;
        }
        self.active = active;
        self.poll_generation += 1;
        if active {
            self.resolve_repo(ctx);
            self.schedule_poll(ctx);
        }
    }

    fn resolve_repo(&mut self, ctx: &mut ViewContext<Self>) {
        let Some(anchor) = self.anchor.clone() else {
            self.repo = None;
            self.state = LoadState::NoRepo;
            ctx.notify();
            return;
        };
        #[cfg(not(target_family = "wasm"))]
        ctx.spawn(
            async move { super::data::main_worktree_for(&anchor).await },
            |view, repo, ctx| {
                if repo.is_none() {
                    view.repo = None;
                    view.worktrees = Arc::new(Vec::new());
                    view.rows = Arc::new(Vec::new());
                    view.state = LoadState::NoRepo;
                    ctx.notify();
                    return;
                }
                if view.repo != repo {
                    view.repo = repo;
                    view.state = LoadState::Loading;
                    view.worktrees = Arc::new(Vec::new());
                    view.rows = Arc::new(Vec::new());
                }
                view.reload(ctx);
            },
        );
        #[cfg(target_family = "wasm")]
        {
            let _ = anchor;
        }
    }

    fn schedule_poll(&mut self, ctx: &mut ViewContext<Self>) {
        let generation = self.poll_generation;
        ctx.spawn(
            async move { warpui::r#async::Timer::after(AGENT_POLL).await },
            move |view, _, ctx| {
                if view.poll_generation != generation || !view.active {
                    return;
                }
                view.polls += 1;
                if view.polls % GIT_POLL_EVERY == 0 {
                    view.reload(ctx);
                } else {
                    view.refresh_agents(ctx);
                }
                view.schedule_poll(ctx);
            },
        );
    }

    fn reload(&mut self, ctx: &mut ViewContext<Self>) {
        let Some(repo) = self.repo.clone() else {
            return;
        };
        if self.loading {
            return;
        }
        self.loading = true;
        #[cfg(not(target_family = "wasm"))]
        ctx.spawn(
            async move {
                let result = super::data::load_worktrees(&repo).await;
                (repo, result)
            },
            |view, (repo, result), ctx| {
                view.loading = false;
                if view.repo.as_deref() != Some(repo.as_path()) {
                    return;
                }
                match result {
                    Ok(worktrees) => view.set_worktrees(worktrees),
                    Err(err) => view.state = LoadState::Failed(format!("{err:#}")),
                }
                ctx.notify();
            },
        );
    }

    /// Re-reads the agent sessions only, keeping the git state.
    fn refresh_agents(&mut self, ctx: &mut ViewContext<Self>) {
        if self.worktrees.is_empty() {
            return;
        }
        #[cfg(not(target_family = "wasm"))]
        {
            let paths: Vec<PathBuf> = self
                .worktrees
                .iter()
                .map(|w| w.entry.path.clone())
                .collect();
            ctx.spawn(
                async move {
                    let sessions = super::data::load_agent_sessions();
                    (paths.clone(), super::data::assign_agents(&paths, sessions))
                },
                |view, (paths, agents), ctx| {
                    let same = view.worktrees.len() == paths.len()
                        && view
                            .worktrees
                            .iter()
                            .zip(&paths)
                            .all(|(w, p)| &w.entry.path == p);
                    if !same {
                        return;
                    }
                    let changed = view
                        .worktrees
                        .iter()
                        .zip(&agents)
                        .any(|(w, a)| &w.agents != a);
                    if changed {
                        let mut worktrees = (*view.worktrees).clone();
                        for (worktree, agents) in worktrees.iter_mut().zip(agents) {
                            worktree.agents = agents;
                        }
                        view.worktrees = Arc::new(worktrees);
                        ctx.notify();
                    }
                },
            );
        }
    }

    fn set_worktrees(&mut self, worktrees: Vec<WorktreeInfo>) {
        self.rows = Arc::new(build_tree(&worktrees));
        if self.card_mouse_states.len() != worktrees.len() {
            self.card_mouse_states = (0..worktrees.len())
                .map(|_| MouseStateHandle::default())
                .collect();
        }
        self.worktrees = Arc::new(worktrees);
        self.state = LoadState::Loaded;
    }

    fn open_menu(&mut self, index: usize, offset: Vector2F, ctx: &mut ViewContext<Self>) {
        let Some(worktree) = self.worktrees.get(index) else {
            return;
        };
        let path = worktree.entry.path.clone();
        let item = |label: &str, action: WorktreesAction| {
            MenuItemFields::new(label)
                .with_on_select_action(action)
                .into_item()
        };
        let mut items = vec![
            item(
                "Open in new tab",
                WorktreesAction::OpenInNewTab(path.clone()),
            ),
            item(
                "Copy path",
                WorktreesAction::CopyText(path.to_string_lossy().into_owned()),
            ),
        ];
        if let Some(branch) = &worktree.entry.branch {
            items.push(item(
                "Copy branch name",
                WorktreesAction::CopyText(branch.clone()),
            ));
        }
        items.push(MenuItem::Separator);
        if !worktree.entry.is_main {
            items.push(item(
                "Remove worktree…",
                WorktreesAction::PromptRemove(path),
            ));
        }
        items.push(item("Prune stale worktrees", WorktreesAction::Prune));
        self.context_menu.update(ctx, move |menu, ctx| {
            menu.set_items(items, ctx);
            ctx.notify();
        });
        self.menu_open = true;
        self.menu_offset = offset;
        ctx.focus(&self.context_menu);
        ctx.notify();
    }

    fn run_git_op(&mut self, remove: Option<PathBuf>, ctx: &mut ViewContext<Self>) {
        let Some(repo) = self.repo.clone() else {
            return;
        };
        #[cfg(not(target_family = "wasm"))]
        ctx.spawn(
            async move {
                match remove {
                    Some(path) => super::data::remove_worktree(&repo, &path).await,
                    None => super::data::prune_worktrees(&repo).await,
                }
            },
            |view, result, ctx| {
                if let Err(err) = result {
                    view.error = Some(format!("{err:#}"));
                }
                view.reload(ctx);
                ctx.notify();
            },
        );
        #[cfg(target_family = "wasm")]
        {
            let _ = (repo, remove);
        }
    }

    fn render_header(&self, appearance: &Appearance) -> Box<dyn Element> {
        let theme = appearance.theme();
        let agents: usize = self.worktrees.iter().map(|w| w.agents.len()).sum();
        let title = match &self.repo {
            Some(repo) => repo
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            None => "Worktrees".to_string(),
        };
        let summary = if self.worktrees.is_empty() {
            String::new()
        } else {
            format!(
                "{} worktree{} · {agents} agent{}",
                self.worktrees.len(),
                plural(self.worktrees.len()),
                plural(agents)
            )
        };
        let refresh = icon_button(
            appearance,
            Icon::Refresh,
            false,
            self.refresh_mouse_state.clone(),
        )
        .build()
        .on_click(|ctx, _, _| ctx.dispatch_typed_action(WorktreesAction::Refresh))
        .finish();
        Container::new(
            Flex::row()
                .with_main_axis_size(MainAxisSize::Max)
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_child(
                    Shrinkable::new(
                        1.0,
                        Text::new_inline(
                            title,
                            appearance.ui_font_family(),
                            appearance.ui_font_size(),
                        )
                        .with_color(theme.foreground().into())
                        .with_style(Properties::default().weight(Weight::Bold))
                        .with_clip(ClipConfig::ellipsis())
                        .finish(),
                    )
                    .finish(),
                )
                .with_child(
                    Container::new(dim_text(summary, appearance, -1.))
                        .with_margin_left(8.)
                        .finish(),
                )
                .with_child(Expanded::new(1.0, Empty::new().finish()).finish())
                .with_child(refresh)
                .finish(),
        )
        .with_horizontal_padding(12.)
        .with_vertical_padding(6.)
        .finish()
    }

    fn render_confirm(&self, path: &Path, appearance: &Appearance) -> Box<dyn Element> {
        let theme = appearance.theme();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let button = |label: &str, state: MouseStateHandle, action: WorktreesAction| {
            appearance
                .ui_builder()
                .button(
                    warpui::ui_components::button::ButtonVariant::Secondary,
                    state,
                )
                .with_text_label(label.to_string())
                .build()
                .on_click(move |ctx, _, _| ctx.dispatch_typed_action(action.clone()))
                .finish()
        };
        let dialog = Container::new(
            ConstrainedBox::new(
                Flex::column()
                    .with_child(
                        Text::new(
                            format!(
                                "Remove worktree \"{name}\"? Its folder is deleted. Git refuses \
                                 if it has uncommitted changes; the branch is kept."
                            ),
                            appearance.ui_font_family(),
                            appearance.ui_font_size(),
                        )
                        .with_color(theme.foreground().into())
                        .finish(),
                    )
                    .with_child(
                        Container::new(
                            Flex::row()
                                .with_main_axis_alignment(MainAxisAlignment::End)
                                .with_main_axis_size(MainAxisSize::Max)
                                .with_child(button(
                                    "Cancel",
                                    self.confirm_mouse_states[0].clone(),
                                    WorktreesAction::CancelRemove,
                                ))
                                .with_child(
                                    Container::new(button(
                                        "Remove",
                                        self.confirm_mouse_states[1].clone(),
                                        WorktreesAction::ConfirmRemove,
                                    ))
                                    .with_margin_left(8.)
                                    .finish(),
                                )
                                .finish(),
                        )
                        .with_margin_top(12.)
                        .finish(),
                    )
                    .finish(),
            )
            .with_max_width(320.)
            .finish(),
        )
        .with_background(theme.surface_2())
        .with_border(warpui::elements::Border::all(1.).with_border_fill(theme.outline()))
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(8.)))
        .with_uniform_padding(14.)
        .finish();
        Dismiss::new(Align::new(dialog).finish())
            .on_dismiss(|ctx, _| ctx.dispatch_typed_action(WorktreesAction::CancelRemove))
            .finish()
    }
}

/// The flowchart: one row per [`TreeRow`], each a connector gutter plus a branch pill or a card.
fn render_tree(
    rows: &[TreeRow],
    worktrees: &[WorktreeInfo],
    card_states: &[MouseStateHandle],
    position_id: &str,
    scroll_state: ClippedScrollStateHandle,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let theme = appearance.theme();
    let line: ColorU = theme.sub_text_color(theme.background()).into();
    let line = ColorU { a: 0x88, ..line };
    let mut column = Flex::column();
    for row in rows {
        let element = match &row.kind {
            NodeKind::Branch(name) => {
                let gutter =
                    TreeGutter::new(row.clone(), BRANCH_ROW_HEIGHT, BRANCH_ROW_HEIGHT / 2.)
                        .with_colors(line, line)
                        .hollow()
                        .finish();
                Flex::row()
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_child(gutter)
                    .with_child(
                        Container::new(branch_pill(name, appearance))
                            .with_margin_left(4.)
                            .finish(),
                    )
                    .finish()
            }
            NodeKind::Worktree(index) => {
                let Some(worktree) = worktrees.get(*index) else {
                    continue;
                };
                let dot = status_color(worktree, appearance);
                let gutter = TreeGutter::new(row.clone(), CARD_HEIGHT + CARD_GAP, CARD_NODE_Y)
                    .with_colors(line, dot)
                    .finish();
                let state = card_states.get(*index).cloned().unwrap_or_default();
                let card = render_card(*index, worktree, state, position_id, appearance);
                Flex::row()
                    .with_cross_axis_alignment(CrossAxisAlignment::Start)
                    .with_child(gutter)
                    .with_child(
                        Expanded::new(
                            1.0,
                            Container::new(card)
                                .with_margin_left(4.)
                                .with_margin_bottom(CARD_GAP)
                                .finish(),
                        )
                        .finish(),
                    )
                    .finish()
            }
        };
        column = column.with_child(element);
    }
    let content = Container::new(column.finish())
        .with_horizontal_padding(10.)
        .with_padding_top(6.)
        .with_padding_right(ScrollbarWidth::Auto.as_f32() + 10.)
        .finish();
    ClippedScrollable::vertical(
        scroll_state,
        content,
        ScrollbarWidth::Auto,
        theme.nonactive_ui_detail().into(),
        theme.active_ui_detail().into(),
        Fill::None,
    )
    .with_overlayed_scrollbar()
    .finish()
}

fn render_card(
    index: usize,
    worktree: &WorktreeInfo,
    state: MouseStateHandle,
    position_id: &str,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let theme = appearance.theme();
    let entry = &worktree.entry;

    // Line 1: name and badges.
    let mut title = Flex::row()
        .with_cross_axis_alignment(CrossAxisAlignment::Center)
        .with_child(
            Shrinkable::new(
                1.0,
                Text::new_inline(
                    entry.name(),
                    appearance.ui_font_family(),
                    appearance.ui_font_size(),
                )
                .with_color(theme.foreground().into())
                .with_style(Properties::default().weight(Weight::Semibold))
                .with_clip(ClipConfig::ellipsis())
                .finish(),
            )
            .finish(),
        );
    let badges = [
        (entry.is_main, "main checkout"),
        (entry.is_agent_worktree(), "agent"),
        (entry.locked, "locked"),
        (entry.prunable, "missing"),
        (entry.branch.is_none() && !entry.prunable, "detached"),
    ];
    for (_, label) in badges.iter().filter(|(on, _)| *on) {
        title = title.with_child(badge(label, appearance));
    }

    // Line 2: branch, ahead/behind, changes.
    let mut git_parts = vec![entry.branch.clone().unwrap_or_else(|| short(&entry.head))];
    if let Some(base) = &worktree.base {
        git_parts.push(format!(
            "↑{} ↓{} vs {base}",
            worktree.ahead, worktree.behind
        ));
    }
    if worktree.changed_files > 0 {
        git_parts.push(format!(
            "{} changed file{}",
            worktree.changed_files,
            plural(worktree.changed_files)
        ));
    } else if !entry.prunable {
        git_parts.push("clean".into());
    }

    // Line 3: last commit.
    let last_commit = if worktree.last_subject.is_empty() {
        String::new()
    } else {
        format!(
            "{} · {}",
            worktree.last_subject,
            relative_time(now_secs() - worktree.last_time)
        )
    };

    let content = Flex::column()
        .with_child(title.finish())
        .with_child(line_text(git_parts.join("  ·  "), appearance, false))
        .with_child(line_text(last_commit, appearance, true))
        .with_child(agents_line(worktree, appearance))
        .finish();

    let path = entry.path.clone();
    let position_id = position_id.to_string();
    let accent = status_color(worktree, appearance);
    let background = theme.surface_2();
    let hover = internal_colors::fg_overlay_2(theme);
    let border = theme.outline();
    Hoverable::new(state, move |mouse| {
        let mut card = Container::new(
            ConstrainedBox::new(content)
                .with_height(CARD_HEIGHT - 2.)
                .finish(),
        )
        .with_background(background)
        .with_border(warpui::elements::Border::all(1.).with_border_fill(border))
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(6.)))
        .with_horizontal_padding(10.)
        .with_vertical_padding(6.);
        if mouse.is_hovered() {
            card = card.with_foreground_overlay(hover);
        }
        Flex::row()
            .with_child(
                ConstrainedBox::new(
                    Container::new(Empty::new().finish())
                        .with_background_color(accent)
                        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(2.)))
                        .finish(),
                )
                .with_width(3.)
                .with_height(CARD_HEIGHT)
                .finish(),
            )
            .with_child(Expanded::new(1.0, card.finish()).finish())
            .finish()
    })
    .on_click(move |ctx, _, _| {
        ctx.dispatch_typed_action(WorktreesAction::OpenInNewTab(path.clone()));
    })
    .on_right_click(move |ctx, _, position| {
        let Some(bounds) = ctx.element_position_by_id(&position_id) else {
            return;
        };
        let offset = position - bounds.origin();
        ctx.dispatch_typed_action(WorktreesAction::OpenMenu {
            index,
            x: offset.x(),
            y: offset.y(),
        });
    })
    .finish()
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

fn short(hash: &str) -> String {
    hash.chars().take(8).collect()
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn dim_text(text: String, appearance: &Appearance, size_delta: f32) -> Box<dyn Element> {
    let theme = appearance.theme();
    Text::new_inline(
        text,
        appearance.ui_font_family(),
        appearance.ui_font_size() + size_delta,
    )
    .with_color(theme.sub_text_color(theme.background()).into())
    .with_clip(ClipConfig::ellipsis())
    .finish()
}

fn line_text(text: String, appearance: &Appearance, dim: bool) -> Box<dyn Element> {
    let theme = appearance.theme();
    let color = if dim {
        theme.sub_text_color(theme.background())
    } else {
        theme.foreground()
    };
    Container::new(
        Text::new_inline(
            text,
            appearance.ui_font_family(),
            appearance.ui_font_size() - 1.,
        )
        .with_color(color.into())
        .with_clip(ClipConfig::ellipsis())
        .finish(),
    )
    .with_margin_top(3.)
    .finish()
}

fn badge(label: &str, appearance: &Appearance) -> Box<dyn Element> {
    let theme = appearance.theme();
    let dim: ColorU = theme.sub_text_color(theme.background()).into();
    Container::new(
        Container::new(
            Text::new_inline(
                label.to_string(),
                appearance.ui_font_family(),
                appearance.ui_font_size() - 2.,
            )
            .with_color(dim)
            .finish(),
        )
        .with_background_color(ColorU { a: 0x22, ..dim })
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(3.)))
        .with_horizontal_padding(5.)
        .with_vertical_padding(1.)
        .finish(),
    )
    .with_margin_left(6.)
    .finish()
}

fn branch_pill(name: &str, appearance: &Appearance) -> Box<dyn Element> {
    let theme = appearance.theme();
    let fg: ColorU = theme.foreground().into();
    Container::new(
        Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(
                ConstrainedBox::new(Icon::GitBranch.to_warpui_icon(fg.into()).finish())
                    .with_width(12.)
                    .with_height(12.)
                    .finish(),
            )
            .with_child(
                Container::new(
                    Text::new_inline(
                        name.to_string(),
                        appearance.ui_font_family(),
                        appearance.ui_font_size() - 1.,
                    )
                    .with_color(fg)
                    .finish(),
                )
                .with_margin_left(5.)
                .finish(),
            )
            .finish(),
    )
    .with_background_color(ColorU { a: 0x1c, ..fg })
    .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)))
    .with_horizontal_padding(8.)
    .with_vertical_padding(3.)
    .finish()
}

/// Green while an agent works, amber when one needs the user, grey otherwise.
fn status_color(worktree: &WorktreeInfo, appearance: &Appearance) -> ColorU {
    let theme = appearance.theme();
    match summary_status(&worktree.agents) {
        Some(AgentStatus::Working) => WORKING,
        Some(AgentStatus::Waiting) => WAITING,
        _ => ColorU {
            a: 0x99,
            ..theme.sub_text_color(theme.background()).into()
        },
    }
}

fn agents_line(worktree: &WorktreeInfo, appearance: &Appearance) -> Box<dyn Element> {
    if worktree.agents.is_empty() {
        return line_text("No agents".into(), appearance, true);
    }
    let theme = appearance.theme();
    let mut row = Flex::row().with_cross_axis_alignment(CrossAxisAlignment::Center);
    let count = worktree.agents.len();
    row = row.with_child(
        Text::new_inline(
            format!("{count} agent{}", plural(count)),
            appearance.ui_font_family(),
            appearance.ui_font_size() - 1.,
        )
        .with_color(theme.foreground().into())
        .with_style(Properties::default().weight(Weight::Semibold))
        .finish(),
    );
    for agent in worktree.agents.iter().take(4) {
        let color = match agent.status {
            AgentStatus::Working => WORKING,
            AgentStatus::Waiting => WAITING,
            _ => theme.sub_text_color(theme.background()).into(),
        };
        let label = if agent.name.is_empty() {
            agent.status.label().to_string()
        } else {
            format!("{} {}", agent.name, agent.status.label())
        };
        row = row.with_child(
            Container::new(
                Flex::row()
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_child(
                        ConstrainedBox::new(
                            Container::new(Empty::new().finish())
                                .with_background_color(color)
                                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(3.)))
                                .finish(),
                        )
                        .with_width(6.)
                        .with_height(6.)
                        .finish(),
                    )
                    .with_child(
                        Container::new(dim_text(label, appearance, -1.))
                            .with_margin_left(4.)
                            .finish(),
                    )
                    .finish(),
            )
            .with_margin_left(10.)
            .finish(),
        );
    }
    if count > 4 {
        row = row.with_child(
            Container::new(dim_text(format!("+{}", count - 4), appearance, -1.))
                .with_margin_left(8.)
                .finish(),
        );
    }
    Container::new(row.finish()).with_margin_top(4.).finish()
}

impl Entity for WorktreesView {
    type Event = WorktreesEvent;
}

impl View for WorktreesView {
    fn ui_name() -> &'static str {
        "WorktreesView"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let mut column = Flex::column().with_child(self.render_header(appearance));
        if let Some(error) = &self.error {
            column = column.with_child(
                Hoverable::new(MouseStateHandle::default(), {
                    let error = error.clone();
                    move |_| {
                        Container::new(
                            Text::new(
                                error.clone(),
                                appearance.ui_font_family(),
                                appearance.ui_font_size() - 1.,
                            )
                            .with_color(appearance.theme().ui_error_color())
                            .finish(),
                        )
                        .with_horizontal_padding(12.)
                        .with_vertical_padding(6.)
                        .finish()
                    }
                })
                .on_click(|ctx, _, _| ctx.dispatch_typed_action(WorktreesAction::DismissError))
                .finish(),
            );
        }
        let body = match &self.state {
            LoadState::NoRepo => message(
                "Open a terminal in a git repository to see its worktrees.",
                appearance,
            ),
            LoadState::Loading => message("Loading worktrees…", appearance),
            LoadState::Failed(err) => {
                message(&format!("Couldn't list worktrees: {err}"), appearance)
            }
            LoadState::Loaded => render_tree(
                &self.rows,
                &self.worktrees,
                &self.card_mouse_states,
                &self.position_id,
                self.scroll_state.clone(),
                appearance,
            ),
        };
        column = column.with_child(Expanded::new(1.0, body).finish());

        let mut stack = Stack::new();
        stack.add_child(SavePosition::new(column.finish(), &self.position_id).finish());
        if self.menu_open {
            stack.add_positioned_overlay_child(
                ChildView::new(&self.context_menu).finish(),
                OffsetPositioning::offset_from_save_position_element(
                    self.position_id.clone(),
                    self.menu_offset,
                    PositionedElementOffsetBounds::ParentByPosition,
                    PositionedElementAnchor::TopLeft,
                    ChildAnchor::TopLeft,
                ),
            );
        }
        if let Some(path) = &self.confirm_remove {
            stack.add_positioned_overlay_child(
                self.render_confirm(path, appearance),
                OffsetPositioning::offset_from_save_position_element(
                    self.position_id.clone(),
                    vec2f(0., 0.),
                    PositionedElementOffsetBounds::ParentByPosition,
                    PositionedElementAnchor::TopLeft,
                    ChildAnchor::TopLeft,
                ),
            );
        }
        stack.finish()
    }
}

fn message(text: &str, appearance: &Appearance) -> Box<dyn Element> {
    Container::new(
        Text::new(
            text.to_string(),
            appearance.ui_font_family(),
            appearance.ui_font_size(),
        )
        .with_color(
            appearance
                .theme()
                .sub_text_color(appearance.theme().background())
                .into(),
        )
        .finish(),
    )
    .with_horizontal_padding(12.)
    .with_vertical_padding(8.)
    .finish()
}

impl TypedActionView for WorktreesView {
    type Action = WorktreesAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            WorktreesAction::Refresh => {
                self.error = None;
                self.resolve_repo(ctx);
            }
            WorktreesAction::OpenInNewTab(path) => {
                self.menu_open = false;
                ctx.emit(WorktreesEvent::OpenDirectoryInNewTab(path.clone()));
            }
            WorktreesAction::OpenMenu { index, x, y } => self.open_menu(*index, vec2f(*x, *y), ctx),
            WorktreesAction::CopyText(text) => {
                self.menu_open = false;
                ctx.clipboard()
                    .write(ClipboardContent::plain_text(text.clone()));
                ctx.notify();
            }
            WorktreesAction::PromptRemove(path) => {
                self.menu_open = false;
                self.confirm_remove = Some(path.clone());
                ctx.notify();
            }
            WorktreesAction::ConfirmRemove => {
                if let Some(path) = self.confirm_remove.take() {
                    self.run_git_op(Some(path), ctx);
                }
                ctx.notify();
            }
            WorktreesAction::CancelRemove => {
                self.confirm_remove = None;
                ctx.notify();
            }
            WorktreesAction::Prune => {
                self.menu_open = false;
                self.run_git_op(None, ctx);
            }
            WorktreesAction::DismissError => {
                self.error = None;
                ctx.notify();
            }
        }
    }
}

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;
