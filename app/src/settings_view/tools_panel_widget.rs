//! The Appearance page's tools panel list: every available panel with a show/hide switch and
//! move up/down buttons, in the order the tools panel shows them.

use std::collections::HashMap;

use settings::Setting as _;
use warp_core::features::FeatureFlag;
use warp_core::ui::Icon;
use warpui::elements::{
    ConstrainedBox, Container, CrossAxisAlignment, Element, Expanded, Flex, MainAxisSize,
    MouseStateHandle, ParentElement, Text,
};
use warpui::ui_components::components::UiComponent;
use warpui::ui_components::switch::SwitchStateHandle;
use warpui::{AppContext, SingletonEntity as _, ViewContext};

use super::appearance_page::{AppearancePageAction, AppearanceSettingsPageView};
use super::settings_page::SettingsWidget;
use crate::appearance::Appearance;
use crate::drive::settings::WarpDriveSettings;
use crate::settings::{CodeSettings, GitSettings};
use crate::ui_components::buttons::icon_button;
use crate::workspace::view::left_panel::sort_by_saved_order;
use crate::{AISettings, report_if_error};

/// One row of the list.
struct ToolsPanelItem {
    /// Matches `ToolPanelView::stable_id`.
    id: &'static str,
    label: &'static str,
    icon: Icon,
    visible: fn(&AppContext) -> bool,
    toggle: AppearancePageAction,
}

/// Every panel the list can show, available or not, so each has persistent mouse state.
const ALL_IDS: &[&str] = &[
    "project_explorer",
    "conversations",
    "global_search",
    "warp_drive",
    "git_graph",
    "worktrees",
    "notes",
];

/// The panels available in this build, in the user's order.
fn items(app: &AppContext) -> Vec<ToolsPanelItem> {
    let mut items = Vec::new();
    if cfg!(feature = "local_fs") {
        items.push(ToolsPanelItem {
            id: "project_explorer",
            label: "Project explorer",
            icon: Icon::FileCopy,
            visible: |app| *CodeSettings::as_ref(app).show_project_explorer,
            toggle: AppearancePageAction::ToggleToolsPanelProjectExplorer,
        });
    }
    if FeatureFlag::AgentViewConversationListView.is_enabled() {
        items.push(ToolsPanelItem {
            id: "conversations",
            label: "Agent conversations",
            icon: Icon::Conversation,
            visible: |app| *AISettings::as_ref(app).show_conversation_history,
            toggle: AppearancePageAction::ToggleToolsPanelConversationHistory,
        });
    }
    if cfg!(feature = "local_fs") && FeatureFlag::GlobalSearch.is_enabled() {
        items.push(ToolsPanelItem {
            id: "global_search",
            label: "Global search",
            icon: Icon::Search,
            visible: |app| *CodeSettings::as_ref(app).show_global_search,
            toggle: AppearancePageAction::ToggleToolsPanelGlobalSearch,
        });
    }
    items.push(ToolsPanelItem {
        id: "warp_drive",
        label: "Warp Drive",
        icon: Icon::WarpDrive,
        visible: |app| *WarpDriveSettings::as_ref(app).enable_warp_drive,
        toggle: AppearancePageAction::ToggleToolsPanelWarpDrive,
    });
    if cfg!(feature = "local_fs") && FeatureFlag::GitGraph.is_enabled() {
        items.push(ToolsPanelItem {
            id: "git_graph",
            label: "Git Graph",
            icon: Icon::GitBranch,
            visible: |app| *GitSettings::as_ref(app).show_git_graph,
            toggle: AppearancePageAction::ToggleToolsPanelGitGraph,
        });
        items.push(ToolsPanelItem {
            id: "worktrees",
            label: "Worktrees",
            icon: Icon::Dataflow,
            visible: |app| *GitSettings::as_ref(app).show_worktrees,
            toggle: AppearancePageAction::ToggleToolsPanelWorktrees,
        });
    }
    if cfg!(feature = "local_fs") {
        items.push(ToolsPanelItem {
            id: "notes",
            label: "Notes",
            icon: Icon::CalendarCheck,
            visible: |app| *CodeSettings::as_ref(app).show_notes,
            toggle: AppearancePageAction::ToggleToolsPanelNotes,
        });
    }
    sort_by_saved_order(
        &mut items,
        |item| item.id,
        &CodeSettings::as_ref(app).tools_panel_order,
    );
    items
}

/// Moves panel `id` one place up or down and saves the order. Ids saved for panels not available
/// in this build stay in the list, after the available ones.
pub(super) fn move_item(id: &str, up: bool, ctx: &mut ViewContext<AppearanceSettingsPageView>) {
    let mut ids: Vec<String> = items(ctx).iter().map(|item| item.id.to_string()).collect();
    let Some(index) = ids.iter().position(|item| item == id) else {
        return;
    };
    let target = if up {
        index.checked_sub(1)
    } else {
        Some(index + 1).filter(|&target| target < ids.len())
    };
    let Some(target) = target else {
        return;
    };
    ids.swap(index, target);
    let saved = CodeSettings::as_ref(ctx).tools_panel_order.value().clone();
    let unavailable: Vec<String> = saved
        .into_iter()
        .filter(|saved_id| !ids.contains(saved_id))
        .collect();
    ids.extend(unavailable);
    CodeSettings::handle(ctx).update(ctx, |settings, ctx| {
        report_if_error!(settings.tools_panel_order.set_value(ids, ctx));
    });
}

pub(super) fn reset_order(ctx: &mut ViewContext<AppearanceSettingsPageView>) {
    CodeSettings::handle(ctx).update(ctx, |settings, ctx| {
        report_if_error!(settings.tools_panel_order.set_value(Vec::new(), ctx));
    });
}

#[derive(Default)]
struct RowStates {
    switch: SwitchStateHandle,
    up: MouseStateHandle,
    down: MouseStateHandle,
}

pub(super) struct ToolsPanelListWidget {
    rows: HashMap<&'static str, RowStates>,
    reset: MouseStateHandle,
}

impl Default for ToolsPanelListWidget {
    fn default() -> Self {
        Self {
            rows: ALL_IDS
                .iter()
                .map(|id| (*id, RowStates::default()))
                .collect(),
            reset: MouseStateHandle::default(),
        }
    }
}

impl SettingsWidget for ToolsPanelListWidget {
    type View = AppearanceSettingsPageView;

    fn search_terms(&self) -> &str {
        "tools panel tabs order reorder sort move up down show hide visibility project explorer \
         file tree global search warp drive agent conversations left panel"
    }

    fn render(
        &self,
        _view: &Self::View,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let items = items(app);
        let last = items.len().saturating_sub(1);
        let mut column = Flex::column().with_child(
            Container::new(
                Text::new(
                    "Show or hide each panel, and set the order of their buttons.".to_string(),
                    appearance.ui_font_family(),
                    appearance.ui_font_size() - 1.,
                )
                .with_color(theme.sub_text_color(theme.background()).into())
                .finish(),
            )
            .with_margin_bottom(6.)
            .finish(),
        );
        for (index, item) in items.iter().enumerate() {
            let Some(states) = self.rows.get(item.id) else {
                continue;
            };
            let arrow = |icon: Icon, state: &MouseStateHandle, up: bool, enabled: bool| {
                let id = item.id;
                let button = icon_button(appearance, icon, false, state.clone()).build();
                if enabled {
                    button
                        .on_click(move |ctx, _, _| {
                            ctx.dispatch_typed_action(AppearancePageAction::MoveToolsPanelItem {
                                id,
                                up,
                            });
                        })
                        .finish()
                } else {
                    Container::new(button.finish())
                        .with_foreground_overlay(theme.background())
                        .finish()
                }
            };
            let toggle = item.toggle.clone();
            let row = Flex::row()
                .with_main_axis_size(MainAxisSize::Max)
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_child(
                    ConstrainedBox::new(
                        item.icon
                            .to_warpui_icon(theme.sub_text_color(theme.background()))
                            .finish(),
                    )
                    .with_width(16.)
                    .with_height(16.)
                    .finish(),
                )
                .with_child(
                    Expanded::new(
                        1.0,
                        Container::new(
                            Text::new_inline(
                                item.label.to_string(),
                                appearance.ui_font_family(),
                                appearance.ui_font_size(),
                            )
                            .with_color(theme.foreground().into())
                            .finish(),
                        )
                        .with_margin_left(10.)
                        .finish(),
                    )
                    .finish(),
                )
                .with_child(
                    Container::new(
                        appearance
                            .ui_builder()
                            .switch(states.switch.clone())
                            .check((item.visible)(app))
                            .build()
                            .on_click(move |ctx, _, _| ctx.dispatch_typed_action(toggle.clone()))
                            .finish(),
                    )
                    .with_margin_right(10.)
                    .finish(),
                )
                .with_child(arrow(Icon::ChevronUp, &states.up, true, index > 0))
                .with_child(arrow(Icon::ChevronDown, &states.down, false, index < last))
                .finish();
            column = column.with_child(Container::new(row).with_vertical_padding(3.).finish());
        }
        if !CodeSettings::as_ref(app).tools_panel_order.is_empty() {
            column = column.with_child(
                Container::new(
                    appearance
                        .ui_builder()
                        .link(
                            "Reset to default order".to_string(),
                            None,
                            Some(Box::new(|ctx| {
                                ctx.dispatch_typed_action(
                                    AppearancePageAction::ResetToolsPanelOrder,
                                );
                            })),
                            self.reset.clone(),
                        )
                        .build()
                        .finish(),
                )
                .with_margin_top(6.)
                .finish(),
            );
        }
        Container::new(column.finish())
            .with_vertical_padding(6.)
            .finish()
    }
}
