//! The uncommitted-changes panel: unstaged and staged file lists with per-file and bulk
//! stage / unstage, per-file discard, and a commit box (summary, description, commit button).

use pathfinder_color::ColorU;
use warp_core::ui::theme::color::internal_colors;
use warpui::ViewHandle;
use warpui::elements::{
    ClippedScrollStateHandle, ClippedScrollable, ConstrainedBox, Container, CornerRadius,
    CrossAxisAlignment, Element, Empty, Expanded, Fill, Flex, Hoverable, MainAxisAlignment,
    MainAxisSize, MouseStateHandle, ParentElement, Radius, ScrollbarWidth, Shrinkable, Text,
};
use warpui::fonts::{Properties, Weight};
use warpui::text_layout::ClipConfig;
use warpui::ui_components::components::{Coords, UiComponent, UiComponentStyles};

use super::data::{ChangedFile, FileStatus};
use super::view::GitGraphAction;
use crate::appearance::Appearance;
use crate::code::editor::{add_color, remove_color};
use crate::editor::EditorView;

/// GitKraken-style soft limit for the summary line.
pub(super) const SUMMARY_SOFT_LIMIT: usize = 72;
const ROW_HEIGHT: f32 = 24.0;
const ICON_SIZE: f32 = 14.0;

/// Mouse states owned by the view, persisted across renders.
#[derive(Default)]
pub(super) struct StagingMouseStates {
    pub stage_all: MouseStateHandle,
    pub unstage_all: MouseStateHandle,
    pub commit: MouseStateHandle,
    /// One per file in [`super::data::CommitDetail::files`].
    pub rows: Vec<FileRowMouseStates>,
}

#[derive(Default, Clone)]
pub(super) struct FileRowMouseStates {
    pub row: MouseStateHandle,
    pub stage: MouseStateHandle,
    pub discard: MouseStateHandle,
}

pub(super) struct StagingPanel<'a> {
    pub files: &'a [ChangedFile],
    pub states: &'a StagingMouseStates,
    /// The summary and description inputs (see [`text_input`]).
    pub summary_input: Box<dyn Element>,
    pub description_input: Box<dyn Element>,
    /// Characters in the summary (the commit button needs one; past the soft limit it warns).
    pub summary_len: usize,
    pub scroll_state: ClippedScrollStateHandle,
}

impl StagingPanel<'_> {
    pub(super) fn render(self, appearance: &Appearance) -> Box<dyn Element> {
        let indexed = |staged: bool| -> Vec<usize> {
            (0..self.files.len())
                .filter(|&i| self.files[i].staged == staged)
                .collect()
        };
        let unstaged = indexed(false);
        let staged = indexed(true);

        let lists = Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_child(self.section(
                "Unstaged Files",
                &unstaged,
                false,
                ("Stage all changes", GitGraphAction::StageAll),
                self.states.stage_all.clone(),
                appearance,
            ))
            .with_child(self.section(
                "Staged Files",
                &staged,
                true,
                ("Unstage all changes", GitGraphAction::UnstageAll),
                self.states.unstage_all.clone(),
                appearance,
            ))
            .finish();
        let theme = appearance.theme();
        let scrollable = ClippedScrollable::vertical(
            self.scroll_state.clone(),
            Container::new(lists)
                .with_padding_right(ScrollbarWidth::Auto.as_f32() + 6.)
                .finish(),
            ScrollbarWidth::Auto,
            theme.nonactive_ui_detail().into(),
            theme.active_ui_detail().into(),
            Fill::None,
        )
        .with_overlayed_scrollbar()
        .finish();

        let staged_count = staged.len();
        let summary_len = self.summary_len;
        let commit_state = self.states.commit.clone();
        let commit_box = commit_box(
            self.summary_input,
            self.description_input,
            summary_len,
            staged_count,
            commit_state,
            appearance,
        );
        Flex::column()
            .with_main_axis_size(MainAxisSize::Max)
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_child(Shrinkable::new(1.0, scrollable).finish())
            .with_child(commit_box)
            .finish()
    }

    fn section(
        &self,
        title: &str,
        indices: &[usize],
        staged: bool,
        (button_label, button_action): (&str, GitGraphAction),
        button_state: MouseStateHandle,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let button_color = if staged {
            theme.foreground().into()
        } else {
            theme.accent().into()
        };
        let header = Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(
                Text::new_inline(
                    format!("{title} ({})", indices.len()),
                    appearance.ui_font_family(),
                    appearance.ui_font_size(),
                )
                .with_color(theme.foreground().into())
                .with_style(Properties::default().weight(Weight::Semibold))
                .finish(),
            )
            .with_child(Expanded::new(1.0, Empty::new().finish()).finish())
            .with_children((!indices.is_empty()).then(|| {
                small_button(
                    button_label,
                    button_state,
                    button_action,
                    button_color,
                    appearance,
                )
            }))
            .finish();
        let mut column = Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_child(
                Container::new(header)
                    .with_vertical_padding(4.)
                    .with_padding_left(6.)
                    .finish(),
            );
        if indices.is_empty() {
            let hint = if staged {
                "Stage files to include them in the next commit."
            } else {
                "No unstaged changes."
            };
            column = column.with_child(
                Container::new(
                    Text::new_inline(
                        hint.to_string(),
                        appearance.ui_font_family(),
                        appearance.ui_font_size() - 1.,
                    )
                    .with_color(theme.sub_text_color(theme.background()).into())
                    .finish(),
                )
                .with_padding_left(6.)
                .with_vertical_padding(4.)
                .finish(),
            );
        }
        for &index in indices {
            column = column.with_child(self.file_row(index, appearance));
        }
        Container::new(column.finish())
            .with_margin_bottom(8.)
            .finish()
    }

    fn file_row(&self, index: usize, appearance: &Appearance) -> Box<dyn Element> {
        let file = &self.files[index];
        let theme = appearance.theme();
        let states = self.states.rows.get(index).cloned().unwrap_or_default();
        let (dir, name) = match file.path.rsplit_once('/') {
            Some((dir, name)) => (dir.to_string(), name.to_string()),
            None => (String::new(), file.path.clone()),
        };
        let icon = crate::code::icon_from_file_path(&name, appearance)
            .unwrap_or_else(|| Empty::new().finish());
        let fg: ColorU = theme.foreground().into();
        let dim: ColorU = theme.sub_text_color(theme.background()).into();
        let font = appearance.ui_font_family();
        let size = appearance.ui_font_size();
        let status = status_badge(file.status, appearance);
        let (label, action, color) = if file.staged {
            ("Unstage", GitGraphAction::UnstageFile(index), fg)
        } else {
            (
                "Stage",
                GitGraphAction::StageFile(index),
                theme.accent().into(),
            )
        };
        let hover_bg = internal_colors::fg_overlay_2(theme);
        let buttons = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(small_button(
                "Discard",
                states.discard,
                GitGraphAction::DiscardFile(index),
                remove_color(appearance),
                appearance,
            ))
            .with_child(
                Container::new(small_button(label, states.stage, action, color, appearance))
                    .with_margin_left(4.)
                    .finish(),
            )
            .finish();
        Hoverable::new(states.row, move |mouse| {
            let mut row = Flex::row()
                .with_main_axis_size(MainAxisSize::Max)
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_child(
                    ConstrainedBox::new(icon)
                        .with_width(ICON_SIZE)
                        .with_height(ICON_SIZE)
                        .finish(),
                )
                .with_child(
                    Container::new(
                        Text::new_inline(name.clone(), font, size)
                            .with_color(fg)
                            .finish(),
                    )
                    .with_margin_left(6.)
                    .finish(),
                )
                .with_child(
                    Shrinkable::new(
                        1.0,
                        Container::new(
                            Text::new_inline(dir.clone(), font, size - 1.)
                                .with_color(dim)
                                .with_clip(ClipConfig::ellipsis())
                                .finish(),
                        )
                        .with_margin_left(6.)
                        .finish(),
                    )
                    .finish(),
                )
                .with_child(Expanded::new(1.0, Empty::new().finish()).finish());
            row = if mouse.is_hovered() {
                row.with_child(buttons)
            } else {
                row.with_child(status)
            };
            let mut container = Container::new(
                ConstrainedBox::new(row.finish())
                    .with_height(ROW_HEIGHT)
                    .finish(),
            )
            .with_horizontal_padding(6.)
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)));
            if mouse.is_hovered() {
                container = container.with_background(hover_bg);
            }
            container.finish()
        })
        .on_click(move |ctx, _, _| ctx.dispatch_typed_action(GitGraphAction::OpenFileDiff(index)))
        .finish()
    }
}

fn commit_box(
    summary: Box<dyn Element>,
    description: Box<dyn Element>,
    summary_len: usize,
    staged_count: usize,
    commit_state: MouseStateHandle,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let theme = appearance.theme();

    let over_limit = summary_len > SUMMARY_SOFT_LIMIT;
    let counter_color: ColorU = if over_limit {
        remove_color(appearance)
    } else {
        theme.sub_text_color(theme.background()).into()
    };
    let label_row = Flex::row()
        .with_main_axis_size(MainAxisSize::Max)
        .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
        .with_child(
            Text::new_inline(
                "Commit summary".to_string(),
                appearance.ui_font_family(),
                appearance.ui_font_size() - 1.,
            )
            .with_color(theme.sub_text_color(theme.background()).into())
            .finish(),
        )
        .with_child(
            Text::new_inline(
                format!("{}/{SUMMARY_SOFT_LIMIT}", summary_len),
                appearance.ui_font_family(),
                appearance.ui_font_size() - 1.,
            )
            .with_color(counter_color)
            .finish(),
        )
        .finish();

    let can_commit = staged_count > 0 && summary_len > 0;
    let label = match (staged_count, summary_len) {
        (0, _) => "Stage files to commit".to_string(),
        (_, 0) => "Type a commit summary".to_string(),
        (1, _) => "Commit changes to 1 file".to_string(),
        (n, _) => format!("Commit changes to {n} files"),
    };
    let commit_button = commit_button(label, commit_state, can_commit, appearance);

    Container::new(
        Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_child(label_row)
            .with_child(Container::new(summary).with_margin_top(4.).finish())
            .with_child(Container::new(description).with_margin_top(6.).finish())
            .with_child(Container::new(commit_button).with_margin_top(8.).finish())
            .finish(),
    )
    .with_border(warpui::elements::Border::top(1.).with_border_fill(theme.outline()))
    .with_horizontal_padding(8.)
    .with_vertical_padding(8.)
    .finish()
}

fn status_badge(status: FileStatus, appearance: &Appearance) -> Box<dyn Element> {
    let theme = appearance.theme();
    let (letter, color): (&str, ColorU) = match status {
        FileStatus::Added => ("A", add_color(appearance)),
        FileStatus::Modified => ("M", theme.ui_warning_color()),
        FileStatus::Deleted => ("D", remove_color(appearance)),
        FileStatus::Renamed => ("R", theme.accent().into()),
    };
    Text::new_inline(
        letter.to_string(),
        appearance.monospace_font_family(),
        appearance.ui_font_size() - 1.,
    )
    .with_color(color)
    .with_style(Properties::default().weight(Weight::Bold))
    .finish()
}

/// A bordered text input around `editor`; `min_height` makes a taller, multi-line box.
pub(super) fn text_input(
    editor: ViewHandle<EditorView>,
    min_height: Option<f32>,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let theme = appearance.theme();
    appearance
        .ui_builder()
        .text_input(editor)
        .with_style(UiComponentStyles {
            border_width: Some(1.),
            border_color: Some(theme.outline().into()),
            border_radius: Some(CornerRadius::with_all(Radius::Pixels(4.))),
            padding: Some(Coords {
                top: 6.,
                bottom: 6.,
                left: 8.,
                right: 8.,
            }),
            height: min_height,
            ..Default::default()
        })
        .build()
        .finish()
}

/// A compact pill button with `color` text on a translucent `color` background.
fn small_button(
    label: &str,
    state: MouseStateHandle,
    action: GitGraphAction,
    color: ColorU,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let label = label.to_string();
    let font = appearance.ui_font_family();
    let size = appearance.ui_font_size() - 2.;
    Hoverable::new(state, move |mouse| {
        let alpha = if mouse.is_hovered() { 0x44 } else { 0x26 };
        Container::new(
            Text::new_inline(label.clone(), font, size)
                .with_color(color)
                .with_style(Properties::default().weight(Weight::Semibold))
                .finish(),
        )
        .with_background_color(ColorU { a: alpha, ..color })
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)))
        .with_horizontal_padding(7.)
        .with_vertical_padding(2.)
        .finish()
    })
    .on_click(move |ctx, _, _| ctx.dispatch_typed_action(action.clone()))
    .finish()
}

fn commit_button(
    label: String,
    state: MouseStateHandle,
    enabled: bool,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let theme = appearance.theme();
    let accent: ColorU = theme.accent().into();
    let dim: ColorU = theme.sub_text_color(theme.background()).into();
    let font = appearance.ui_font_family();
    let size = appearance.ui_font_size();
    let hoverable = Hoverable::new(state, move |mouse| {
        let (bg, fg) = if enabled {
            let alpha = if mouse.is_hovered() { 0xff } else { 0xe0 };
            (
                ColorU { a: alpha, ..accent },
                warp_core::ui::color::pick_foreground_color(accent),
            )
        } else {
            (ColorU { a: 0x22, ..dim }, dim)
        };
        Container::new(
            Flex::row()
                .with_main_axis_size(MainAxisSize::Max)
                .with_main_axis_alignment(MainAxisAlignment::Center)
                .with_child(
                    Text::new_inline(label.clone(), font, size)
                        .with_color(fg)
                        .with_style(Properties::default().weight(Weight::Semibold))
                        .finish(),
                )
                .finish(),
        )
        .with_background_color(bg)
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(5.)))
        .with_vertical_padding(7.)
        .finish()
    });
    if enabled {
        hoverable
            .on_click(|ctx, _, _| ctx.dispatch_typed_action(GitGraphAction::Commit))
            .finish()
    } else {
        hoverable.finish()
    }
}

#[cfg(test)]
#[path = "staging_tests.rs"]
mod tests;
