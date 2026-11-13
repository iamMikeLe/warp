//! Column layout for the commit list: **Branch / Tag | Graph | Commit message**.
//!
//! The header owns two [`ResizableStateHandle`]s (Branch / Tag and Graph widths);
//! every row reads the same handles, so dragging a header edge resizes the
//! column for all rows. The message column takes the remaining width.

use pathfinder_color::ColorU;
use warp_core::ui::Icon;
use warp_core::ui::theme::color::internal_colors;
use warpui::elements::{
    Border, ConstrainedBox, Container, CornerRadius, CrossAxisAlignment, DragBarSide, Element,
    Empty, Expanded, Fill, Flex, Highlight, Hoverable, MainAxisSize, MouseStateHandle,
    ParentElement, Radius, Resizable, ResizableStateHandle, ScrollbarWidth, Shrinkable, Text,
};
use warpui::text_layout::ClipConfig;

use super::data::{CommitNode, RefKind, RefLabel};
use super::layout::GraphRow;
use super::row_canvas::{GitGraphRowCanvas, lane_color};
use super::view::{GitGraphAction, ref_menu_kind};
use crate::appearance::Appearance;

/// Default Branch / Tag column width.
pub(super) const DEFAULT_REFS_WIDTH: f32 = 170.0;
/// Default Graph column width.
pub(super) const DEFAULT_GRAPH_WIDTH: f32 = 160.0;
const MIN_REFS_WIDTH: f32 = 60.0;
const MIN_GRAPH_WIDTH: f32 = 32.0;
/// Shortest connector kept between a ref label and the graph, so the link to the
/// commit stays visible even when the label fills the column.
const MIN_CONNECTOR: f32 = 10.0;
/// Faint tint of a commit's lane color behind its whole row, so a row's label, lane, and message
/// read as one unit.
const ROW_TINT_ALPHA: u8 = 0x1c;
const HEADER_HEIGHT: f32 = 24.0;
const CELL_PADDING: f32 = 6.0;

/// Column widths for one frame, read from the header's resizable states.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ColumnWidths {
    pub refs: f32,
    pub graph: f32,
}

impl ColumnWidths {
    pub(super) fn read(refs: &ResizableStateHandle, graph: &ResizableStateHandle) -> Self {
        let size = |state: &ResizableStateHandle, default: f32| {
            state.lock().map(|s| s.size()).unwrap_or(default)
        };
        Self {
            refs: size(refs, DEFAULT_REFS_WIDTH),
            graph: size(graph, DEFAULT_GRAPH_WIDTH),
        }
    }
}

/// The ref shown in a row's Branch / Tag cell, plus how many others the row has.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct PrimaryRef<'a> {
    pub label: &'a RefLabel,
    /// A remote-tracking branch with the same name as the shown local branch
    /// (`main` + `origin/main`): drawn as a second icon on the same label instead
    /// of counting toward the overflow.
    pub tracked_remote: Option<&'a RefLabel>,
    /// Refs not shown in the cell (they're listed in the commit details).
    pub hidden: usize,
}

fn ref_priority(kind: RefKind) -> u8 {
    match kind {
        RefKind::Head => 0,
        RefKind::LocalBranch => 1,
        RefKind::RemoteBranch => 2,
        RefKind::Tag => 3,
        RefKind::Stash => 4,
    }
}

/// Picks the ref to show: the current branch first, then local branches, remote
/// branches, tags, and stashes, keeping git's order within each kind.
pub(super) fn primary_ref(refs: &[RefLabel]) -> Option<PrimaryRef<'_>> {
    let label = refs.iter().min_by_key(|r| ref_priority(r.kind))?;
    let tracked_remote = match label.kind {
        RefKind::Head | RefKind::LocalBranch => refs.iter().find(|r| {
            r.kind == RefKind::RemoteBranch
                && r.name.split_once('/').map(|(_, branch)| branch) == Some(label.name.as_str())
        }),
        _ => None,
    };
    let shown = 1 + usize::from(tracked_remote.is_some());
    Some(PrimaryRef {
        label,
        tracked_remote,
        hidden: refs.len().saturating_sub(shown),
    })
}

fn ref_icon(kind: RefKind) -> Icon {
    match kind {
        RefKind::Head => Icon::Check,
        RefKind::LocalBranch => Icon::Laptop,
        RefKind::RemoteBranch => Icon::Cloud,
        RefKind::Tag => Icon::Bookmark,
        RefKind::Stash => Icon::Inbox,
    }
}

fn small_icon(icon: Icon, color: ColorU) -> Box<dyn Element> {
    ConstrainedBox::new(icon.to_warpui_icon(color.into()).finish())
        .with_width(12.)
        .with_height(12.)
        .finish()
}

fn header_label(text: &str, appearance: &Appearance) -> Box<dyn Element> {
    let theme = appearance.theme();
    Container::new(
        Text::new_inline(
            text.to_string(),
            appearance.ui_font_family(),
            appearance.ui_font_size() - 2.,
        )
        .with_color(theme.sub_text_color(theme.background()).into())
        .with_clip(ClipConfig::ellipsis())
        .finish(),
    )
    .with_padding_left(CELL_PADDING)
    .finish()
}

/// The column header row. Dragging the right edge of "Branch / Tag" or "Graph"
/// resizes that column for every row.
pub(super) fn render_column_header(
    refs_state: &ResizableStateHandle,
    graph_state: &ResizableStateHandle,
    widths: ColumnWidths,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let hover: Fill = internal_colors::fg_overlay_5(appearance.theme()).into();
    // `Resizable` only caps its child's width, so the cell has to claim the full column width
    // itself to line up with the rows.
    let resizable_cell = |state: &ResizableStateHandle, width: f32, text: &str, min: f32| {
        Resizable::new(
            state.clone(),
            ConstrainedBox::new(header_label(text, appearance))
                .with_width(width)
                .with_height(HEADER_HEIGHT)
                .finish(),
        )
        .with_dragbar_side(DragBarSide::Right)
        .with_dragbar_hover_color(hover)
        .on_resize(|ctx, _| ctx.notify())
        .with_bounds_callback(Box::new(move |window_size| {
            (min, (window_size.x() * 0.5).max(min))
        }))
        .finish()
    };
    let theme = appearance.theme();
    Container::new(
        Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(resizable_cell(
                refs_state,
                widths.refs,
                "BRANCH / TAG",
                MIN_REFS_WIDTH,
            ))
            .with_child(resizable_cell(
                graph_state,
                widths.graph,
                "GRAPH",
                MIN_GRAPH_WIDTH,
            ))
            .with_child(Expanded::new(1.0, header_label("COMMIT MESSAGE", appearance)).finish())
            .finish(),
    )
    .with_border(Border::bottom(1.).with_border_fill(theme.outline()))
    .finish()
}

/// A ref pill in `color`: an icon for the ref kind, a second (remote) icon when
/// `tracked_remote` is set, and the name ellipsized to fit. The current branch is drawn filled.
pub(super) fn ref_pill(
    label: &RefLabel,
    tracked_remote: bool,
    color: ColorU,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let is_current = label.kind == RefKind::Head;
    let (bg, fg) = if is_current {
        (color, warp_core::ui::color::pick_foreground_color(color))
    } else {
        (ColorU { a: 0x33, ..color }, color)
    };
    let mut content = Flex::row()
        .with_cross_axis_alignment(CrossAxisAlignment::Center)
        .with_child(small_icon(ref_icon(label.kind), fg));
    if tracked_remote {
        content = content.with_child(
            Container::new(small_icon(Icon::Cloud, fg))
                .with_padding_left(2.)
                .finish(),
        );
    }
    content = content.with_child(
        Shrinkable::new(
            1.0,
            Container::new(
                Text::new_inline(
                    label.name.clone(),
                    appearance.ui_font_family(),
                    appearance.ui_font_size(),
                )
                .with_color(fg.into())
                .with_clip(ClipConfig::ellipsis())
                .finish(),
            )
            .with_padding_left(4.)
            .finish(),
        )
        .finish(),
    );
    Container::new(content.finish())
        .with_background_color(bg)
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(3.)))
        .with_horizontal_padding(5.)
        .with_vertical_padding(2.)
        .finish()
}

/// The row's ref pill in its commit's lane color, with the ref's right-click menu.
fn render_ref_label(
    primary: &PrimaryRef<'_>,
    color: ColorU,
    menu_target: (usize, &str),
    appearance: &Appearance,
) -> Box<dyn Element> {
    let label = primary.label;
    let pill = ref_pill(label, primary.tracked_remote.is_some(), color, appearance);

    let (index, position_id) = menu_target;
    let position_id = position_id.to_string();
    let menu_kind = ref_menu_kind(label.kind, label.name.clone(), index);
    Hoverable::new(MouseStateHandle::default(), move |_| pill)
        .on_right_click(move |ctx, _app, position| {
            let Some(bounds) = ctx.element_position_by_id(&position_id) else {
                return;
            };
            let offset = position - bounds.origin();
            ctx.dispatch_typed_action(GitGraphAction::OpenMenu {
                kind: menu_kind.clone(),
                x: offset.x(),
                y: offset.y(),
            });
        })
        .finish()
}

fn render_overflow_chip(hidden: usize, appearance: &Appearance) -> Box<dyn Element> {
    let theme = appearance.theme();
    let dim = theme.sub_text_color(theme.background());
    Container::new(
        Container::new(
            Text::new_inline(
                format!("+{hidden}"),
                appearance.ui_font_family(),
                appearance.ui_font_size() - 1.,
            )
            .with_color(dim.into())
            .finish(),
        )
        .with_background_color(ColorU {
            a: 0x22,
            ..dim.into()
        })
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(3.)))
        .with_horizontal_padding(4.)
        .with_vertical_padding(1.)
        .finish(),
    )
    .with_padding_left(3.)
    .finish()
}

fn connector_line(color: ColorU, width: Option<f32>) -> Box<dyn Element> {
    let mut line = ConstrainedBox::new(Empty::new().finish()).with_height(2.);
    if let Some(width) = width {
        line = line.with_width(width);
    }
    Container::new(line.finish())
        .with_background_color(ColorU { a: 0x99, ..color })
        .finish()
}

/// The Branch / Tag cell: the row's primary ref, a "+N" chip for the rest, and a
/// connector to the graph. Empty when the commit has no refs.
fn render_refs_cell(
    commit: &CommitNode,
    color: ColorU,
    index: usize,
    position_id: &str,
    width: f32,
    appearance: &Appearance,
) -> (Box<dyn Element>, Option<ColorU>) {
    let Some(primary) = primary_ref(&commit.refs) else {
        return (
            ConstrainedBox::new(Empty::new().finish())
                .with_width(width)
                .finish(),
            None,
        );
    };
    // Flexible children are laid out in order from the space left after the fixed ones, so the
    // label's large flex lets it take what it needs and the connector gets the remainder.
    let mut row = Flex::row()
        .with_main_axis_size(MainAxisSize::Max)
        .with_cross_axis_alignment(CrossAxisAlignment::Center)
        .with_child(
            Shrinkable::new(
                1000.0,
                render_ref_label(&primary, color, (index, position_id), appearance),
            )
            .finish(),
        );
    if primary.hidden > 0 {
        row = row.with_child(render_overflow_chip(primary.hidden, appearance));
    }
    row = row
        .with_child(connector_line(color, Some(MIN_CONNECTOR)))
        .with_child(Expanded::new(1.0, connector_line(color, None)).finish());
    let cell = ConstrainedBox::new(
        Container::new(row.finish())
            .with_padding_left(CELL_PADDING)
            .finish(),
    )
    .with_width(width)
    .finish();
    (cell, Some(color))
}

/// The Commit message cell: the subject, then the first body line dimmed.
fn render_message_cell(
    subject: &str,
    body_preview: &str,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let theme = appearance.theme();
    let font = appearance.ui_font_family();
    let size = appearance.ui_font_size();
    let text = if body_preview.is_empty() {
        Text::new_inline(subject.to_string(), font, size)
            .with_color(theme.foreground().into())
            .with_clip(ClipConfig::ellipsis())
            .finish()
    } else {
        // One clipped line: the subject in the foreground color, the body
        // preview dimmed after it.
        let dim = theme.sub_text_color(theme.background());
        let full = format!("{subject}   {body_preview}");
        let subject_len = subject.chars().count();
        Text::new_inline(full, font, size)
            .with_color(dim.into())
            .with_single_highlight(
                Highlight::new().with_foreground_color(theme.foreground().into()),
                (0..subject_len).collect(),
            )
            .with_clip(ClipConfig::ellipsis())
            .finish()
    };
    Container::new(text)
        .with_padding_left(CELL_PADDING)
        // Keep clear of the list's overlay scrollbar.
        .with_padding_right(ScrollbarWidth::Auto.as_f32() + CELL_PADDING)
        .finish()
}

/// One commit row across all three columns.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_commit_row(
    row: &GraphRow,
    lane_count: usize,
    commit: &CommitNode,
    index: usize,
    position_id: &str,
    widths: ColumnWidths,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let color = lane_color(row.node_color);
    let (refs_cell, connector_color) =
        render_refs_cell(commit, color, index, position_id, widths.refs, appearance);
    let cells = Flex::row()
        .with_main_axis_size(MainAxisSize::Max)
        .with_cross_axis_alignment(CrossAxisAlignment::Center)
        .with_child(refs_cell)
        .with_child(
            GitGraphRowCanvas::new(row.clone(), lane_count, false)
                .with_width(widths.graph)
                .with_ref_connector(connector_color)
                .finish(),
        )
        .with_child(
            Expanded::new(
                1.0,
                render_message_cell(&commit.subject, &commit.body_preview, appearance),
            )
            .finish(),
        )
        .finish();
    Container::new(cells)
        .with_background_color(ColorU {
            a: ROW_TINT_ALPHA,
            ..color
        })
        .finish()
}

/// The synthetic uncommitted-changes row ("WIP"), drawn with a hollow node.
pub(super) fn render_wip_row(
    row: &GraphRow,
    lane_count: usize,
    changed_files: usize,
    widths: ColumnWidths,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let theme = appearance.theme();
    let noun = if changed_files == 1 { "file" } else { "files" };
    let text = format!("WIP   {changed_files} changed {noun}");
    let wip_len = "WIP".len();
    let message = Container::new(
        Text::new_inline(text, appearance.ui_font_family(), appearance.ui_font_size())
            .with_color(theme.sub_text_color(theme.background()).into())
            .with_single_highlight(
                Highlight::new().with_foreground_color(theme.foreground().into()),
                (0..wip_len).collect(),
            )
            .with_clip(ClipConfig::ellipsis())
            .finish(),
    )
    .with_padding_left(CELL_PADDING)
    .finish();
    Flex::row()
        .with_main_axis_size(MainAxisSize::Max)
        .with_cross_axis_alignment(CrossAxisAlignment::Center)
        .with_child(
            ConstrainedBox::new(Empty::new().finish())
                .with_width(widths.refs)
                .finish(),
        )
        .with_child(
            GitGraphRowCanvas::new(row.clone(), lane_count, true)
                .with_width(widths.graph)
                .finish(),
        )
        .with_child(Expanded::new(1.0, message).finish())
        .finish()
}

#[cfg(test)]
#[path = "columns_tests.rs"]
mod tests;
