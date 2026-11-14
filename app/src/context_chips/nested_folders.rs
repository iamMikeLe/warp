//! Cascading folder panels for the working-directory menu: hovering a folder opens a panel of its
//! subfolders next to it, hovering a subfolder opens the next level, and clicking any of them
//! changes to that folder.

use std::collections::HashMap;

use pathfinder_geometry::vector::vec2f;
use warp_core::ui::appearance::Appearance;
use warp_core::ui::theme::Fill;
use warp_core::ui::theme::color::internal_colors;
use warpui::elements::{
    ChildAnchor, ClippedScrollStateHandle, ClippedScrollable, ConstrainedBox, Container,
    CornerRadius, CrossAxisAlignment, DispatchEventResult, DropShadow, EventHandler, Flex,
    Hoverable, MainAxisSize, MouseInBehavior, MouseStateHandle, OffsetPositioning, ParentElement,
    PositionedElementAnchor, PositionedElementOffsetBounds, Radius, SavePosition, ScrollbarWidth,
    Shrinkable, Text,
};
use warpui::text_layout::ClipConfig;
use warpui::{AppContext, Element, SingletonEntity as _};

use super::directory_fetcher::{DirectoryType, list_directory};
use super::display_menu::DisplayChipMenuAction;
use crate::completer::SessionContext;
use crate::ui_components::icons::Icon;

/// Panels deeper than this don't open, to keep the cascade on screen.
const MAX_DEPTH: usize = 6;
const PANEL_WIDTH: f32 = 220.;
const PANEL_MAX_HEIGHT: f32 = 280.;

/// Where nested folders are listed from: the session (local or remote) and the directory the
/// working-directory menu shows.
#[derive(Clone)]
pub struct NestedFolderSource {
    pub session_context: SessionContext,
    pub base_dir: String,
}

/// One open panel: the subfolders of `rel_path` (relative to the base directory).
pub(super) struct FolderPanel {
    pub rel_path: String,
    /// Save-position id of the row this panel opened from.
    anchor_id: String,
    /// `None` while loading.
    folders: Option<Vec<String>>,
    mouse_states: Vec<MouseStateHandle>,
    scroll_state: ClippedScrollStateHandle,
}

/// The open panels and a cache of listed folders.
#[derive(Default)]
pub(super) struct NestedFolders {
    pub source: Option<NestedFolderSource>,
    pub panels: Vec<FolderPanel>,
    cache: HashMap<String, Vec<String>>,
}

/// What hovering a row asks for.
pub(super) enum HoverOutcome {
    /// Nothing changed (the panel is already open, or too deep).
    Unchanged,
    /// Panels were closed or opened from the cache.
    Updated,
    /// A panel opened for `rel_path` and needs its folders listed.
    Load {
        rel_path: String,
        source: NestedFolderSource,
    },
}

pub(super) fn join(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    }
}

impl NestedFolders {
    pub fn set_source(&mut self, source: Option<NestedFolderSource>) {
        self.source = source;
        self.reset();
    }

    /// Closes every panel and forgets listed folders (the directory may have changed).
    pub fn reset(&mut self) {
        self.panels.clear();
        self.cache.clear();
    }

    pub fn close_from(&mut self, level: usize) {
        self.panels.truncate(level);
    }

    /// The folder name of row `index` in panel `level` (1-based: panel 1 opens from the menu).
    pub fn folder_in_panel(&self, level: usize, index: usize) -> Option<(String, String)> {
        let panel = self.panels.get(level.checked_sub(1)?)?;
        let name = panel.folders.as_ref()?.get(index)?.clone();
        Some((join(&panel.rel_path, &name), anchor_id(level, index)))
    }

    /// Hovering `folder` (relative path) whose row is `anchor_id` in panel `level` (0 = the menu
    /// itself): closes deeper panels and opens one for `folder`.
    pub fn hover(&mut self, level: usize, folder: String, anchor_id: String) -> HoverOutcome {
        if self
            .panels
            .get(level)
            .is_some_and(|panel| panel.rel_path == folder)
        {
            return HoverOutcome::Unchanged;
        }
        let Some(source) = self.source.clone() else {
            return HoverOutcome::Unchanged;
        };
        self.panels.truncate(level);
        if level >= MAX_DEPTH {
            return HoverOutcome::Updated;
        }
        let cached = self.cache.get(&folder).cloned();
        let loaded = cached.is_some();
        self.panels.push(FolderPanel {
            mouse_states: cached
                .as_ref()
                .map(|folders| {
                    folders
                        .iter()
                        .map(|_| MouseStateHandle::default())
                        .collect()
                })
                .unwrap_or_default(),
            folders: cached,
            rel_path: folder.clone(),
            anchor_id,
            scroll_state: ClippedScrollStateHandle::new(),
        });
        if loaded {
            HoverOutcome::Updated
        } else {
            HoverOutcome::Load {
                rel_path: folder,
                source,
            }
        }
    }

    /// Stores a finished listing; fills the panel if it's still open.
    pub fn loaded(&mut self, rel_path: String, folders: Vec<String>) {
        if let Some(panel) = self
            .panels
            .iter_mut()
            .find(|panel| panel.rel_path == rel_path)
        {
            panel.mouse_states = folders
                .iter()
                .map(|_| MouseStateHandle::default())
                .collect();
            panel.folders = Some(folders.clone());
        }
        self.cache.insert(rel_path, folders);
    }

    /// Renders every open panel with where to place it.
    pub fn render(&self, app: &AppContext) -> Vec<(Box<dyn Element>, OffsetPositioning)> {
        self.panels
            .iter()
            .enumerate()
            .map(|(index, panel)| {
                let open_child = self
                    .panels
                    .get(index + 1)
                    .map(|child| child.anchor_id.as_str());
                let positioning = OffsetPositioning::offset_from_save_position_element(
                    panel.anchor_id.clone(),
                    vec2f(2., -4.),
                    PositionedElementOffsetBounds::WindowByPosition,
                    PositionedElementAnchor::TopRight,
                    ChildAnchor::TopLeft,
                );
                (render_panel(panel, index + 1, open_child, app), positioning)
            })
            .collect()
    }
}

/// Save-position id of row `index` in panel `level`.
pub(super) fn anchor_id(level: usize, index: usize) -> String {
    format!("NestedFolder-{level}-{index}")
}

/// Lists the subfolders of `rel_path` under the source's directory, hidden ones excluded.
pub(super) async fn list_subfolders(source: &NestedFolderSource, rel_path: &str) -> Vec<String> {
    let separator = source.session_context.session.path_separators().main;
    let base = source.base_dir.trim_end_matches(['/', '\\']);
    let dir = format!(
        "{base}{separator}{}",
        rel_path.replace('/', &separator.to_string())
    );
    list_directory(&source.session_context, &dir)
        .await
        .into_iter()
        .filter(|item| item.directory_type == DirectoryType::Directory)
        .map(|item| item.name)
        .collect()
}

fn render_panel(
    panel: &FolderPanel,
    level: usize,
    open_child: Option<&str>,
    app: &AppContext,
) -> Box<dyn Element> {
    let appearance = Appearance::as_ref(app);
    let theme = appearance.theme();
    let background = theme.surface_2();
    let text = theme.main_text_color(background).into_solid();
    let dim = theme.sub_text_color(background).into_solid();
    let font = appearance.ui_font_family();
    let size = appearance.ui_font_size();

    let message = |label: &str| {
        Container::new(
            Text::new_inline(label.to_string(), font, size)
                .with_color(dim)
                .finish(),
        )
        .with_horizontal_padding(10.)
        .with_vertical_padding(6.)
        .finish()
    };
    let body: Box<dyn Element> = match &panel.folders {
        None => message("Loading…"),
        Some(folders) if folders.is_empty() => message("No subfolders"),
        Some(folders) => {
            let mut column = Flex::column().with_cross_axis_alignment(CrossAxisAlignment::Stretch);
            for (index, name) in folders.iter().enumerate() {
                let id = anchor_id(level, index);
                let is_open = open_child == Some(id.as_str());
                let state = panel.mouse_states.get(index).cloned().unwrap_or_default();
                let name = name.clone();
                let row = Hoverable::new(state, move |mouse| {
                    let icon = |icon: Icon, color| {
                        ConstrainedBox::new(icon.to_warpui_icon(Fill::Solid(color)).finish())
                            .with_width(14.)
                            .with_height(14.)
                            .finish()
                    };
                    let mut row = Container::new(
                        Flex::row()
                            .with_main_axis_size(MainAxisSize::Max)
                            .with_cross_axis_alignment(CrossAxisAlignment::Center)
                            .with_child(icon(Icon::Folder, dim))
                            .with_child(
                                Shrinkable::new(
                                    1.0,
                                    Container::new(
                                        Text::new_inline(name.clone(), font, size)
                                            .with_color(text)
                                            .with_clip(ClipConfig::ellipsis())
                                            .finish(),
                                    )
                                    .with_horizontal_padding(6.)
                                    .finish(),
                                )
                                .finish(),
                            )
                            .with_child(icon(Icon::ChevronRight, dim))
                            .finish(),
                    )
                    .with_horizontal_padding(8.)
                    .with_vertical_padding(5.)
                    .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)));
                    if mouse.is_hovered() || is_open {
                        row = row.with_background(internal_colors::fg_overlay_3(theme));
                    }
                    row.finish()
                })
                .finish();
                let row = EventHandler::new(row)
                    .on_left_mouse_down(move |ctx, _, _| {
                        ctx.dispatch_typed_action(DisplayChipMenuAction::OpenNestedFolder {
                            level,
                            index,
                        });
                        DispatchEventResult::StopPropagation
                    })
                    .on_mouse_in(
                        move |ctx, _, _| {
                            ctx.dispatch_typed_action(DisplayChipMenuAction::HoverNestedFolder {
                                level,
                                index,
                            });
                            DispatchEventResult::StopPropagation
                        },
                        Some(MouseInBehavior {
                            fire_on_synthetic_events: false,
                            fire_when_covered: false,
                        }),
                    )
                    .finish();
                column = column.with_child(SavePosition::new(row, &id).finish());
            }
            ClippedScrollable::vertical(
                panel.scroll_state.clone(),
                column.finish(),
                ScrollbarWidth::Auto,
                theme.nonactive_ui_detail().into(),
                theme.active_ui_detail().into(),
                warpui::elements::Fill::None,
            )
            .with_overlayed_scrollbar()
            .finish()
        }
    };
    ConstrainedBox::new(
        Container::new(body)
            .with_uniform_padding(4.)
            .with_background(background)
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(6.)))
            .with_drop_shadow(DropShadow::default())
            .finish(),
    )
    .with_width(PANEL_WIDTH)
    .with_max_height(PANEL_MAX_HEIGHT)
    .finish()
}

#[cfg(test)]
#[path = "nested_folders_tests.rs"]
mod tests;
