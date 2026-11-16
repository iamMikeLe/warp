//! The Notes panel: today's running todo list, with finished tasks moved to a collapsible
//! archive grouped by the day they were done.

use std::collections::HashMap;

use chrono::{Duration, Local, NaiveDate};
use warp_core::ui::Icon;
use warp_core::ui::theme::color::internal_colors;
use warpui::elements::{
    ChildAnchor, ChildView, ClippedScrollStateHandle, ClippedScrollable, ConstrainedBox, Container,
    CornerRadius, CrossAxisAlignment, Element, Empty, Expanded, Fill, Flex, Highlight, Hoverable,
    MainAxisSize, MouseStateHandle, OffsetPositioning, ParentElement, PositionedElementAnchor,
    PositionedElementOffsetBounds, Radius, SavePosition, ScrollbarWidth, Shrinkable, Stack, Text,
};
use warpui::fonts::{Properties, Weight};
use warpui::geometry::vector::{Vector2F, vec2f};
use warpui::platform::Cursor;
use warpui::text_layout::TextStyle;
use warpui::ui_components::button::ButtonVariant;
use warpui::ui_components::components::{Coords, UiComponent, UiComponentStyles};
use warpui::{
    AppContext, Entity, FocusContext, ModelHandle, SingletonEntity, TypedActionView, View,
    ViewContext, ViewHandle,
};

use super::data::{Task, TaskId};
use super::model::{NotesEvent, NotesModel};
use crate::appearance::Appearance;
use crate::editor::{EditorView, Event as EditorEvent, SingleLineEditorOptions, TextOptions};
use crate::menu::{Menu, MenuItem, MenuItemFields};

const CONTEXT_MENU_WIDTH: f32 = 180.0;
const CHECKBOX_SIZE: f32 = 14.0;
const ROW_SPACING: f32 = 2.0;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum NotesAction {
    ToggleDone(TaskId),
    OpenMenu { id: TaskId, x: f32, y: f32 },
    StartEdit(TaskId),
    Delete(TaskId),
    SetArchived { id: TaskId, archived: bool },
    ArchiveDone,
    ToggleArchivedSection,
}

#[derive(Default)]
struct TaskMouseStates {
    row: MouseStateHandle,
    checkbox: MouseStateHandle,
}

pub(crate) struct NotesView {
    model: ModelHandle<NotesModel>,
    new_task_input: ViewHandle<EditorView>,
    edit_input: ViewHandle<EditorView>,
    editing: Option<TaskId>,
    task_mouse_states: HashMap<TaskId, TaskMouseStates>,
    archive_done_mouse_state: MouseStateHandle,
    archived_header_mouse_state: MouseStateHandle,
    show_archived: bool,
    scroll_state: ClippedScrollStateHandle,
    position_id: String,
    context_menu: ViewHandle<Menu<NotesAction>>,
    menu_open: bool,
    menu_offset: Vector2F,
}

impl NotesView {
    pub(crate) fn new(ctx: &mut ViewContext<Self>) -> Self {
        // Every window's panel shows the same list, so the first panel creates the shared model.
        if !ctx.has_singleton_model::<NotesModel>() {
            ctx.add_singleton_model(|_| NotesModel::load());
        }
        let model = NotesModel::handle(ctx);
        ctx.subscribe_to_model(&model, |me, _, event, ctx| match event {
            NotesEvent::Changed => me.on_model_changed(ctx),
        });

        let new_task_input = Self::single_line_editor("Add a task…", ctx);
        ctx.subscribe_to_view(&new_task_input, |me, editor, event, ctx| match event {
            EditorEvent::Enter => {
                let text = editor.as_ref(ctx).buffer_text(ctx);
                me.model.update(ctx, |model, ctx| model.add(&text, ctx));
                editor.update(ctx, |editor, ctx| editor.set_buffer_text("", ctx));
            }
            EditorEvent::Escape => {
                editor.update(ctx, |editor, ctx| editor.set_buffer_text("", ctx));
            }
            _ => {}
        });

        let edit_input = Self::single_line_editor("Task", ctx);
        ctx.subscribe_to_view(&edit_input, |me, editor, event, ctx| match event {
            EditorEvent::Enter => {
                if let Some(id) = me.editing.take() {
                    let text = editor.as_ref(ctx).buffer_text(ctx);
                    me.model
                        .update(ctx, |model, ctx| model.edit(id, &text, ctx));
                    ctx.focus(&me.new_task_input);
                    ctx.notify();
                }
            }
            EditorEvent::Escape => me.cancel_edit(ctx),
            _ => {}
        });

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

        let mut view = Self {
            model,
            new_task_input,
            edit_input,
            editing: None,
            task_mouse_states: HashMap::new(),
            archive_done_mouse_state: MouseStateHandle::default(),
            archived_header_mouse_state: MouseStateHandle::default(),
            show_archived: false,
            scroll_state: ClippedScrollStateHandle::new(),
            position_id: format!("notes_{}", ctx.view_id()),
            context_menu,
            menu_open: false,
            menu_offset: Vector2F::zero(),
        };
        view.sync_mouse_states(ctx);
        view
    }

    fn single_line_editor(
        placeholder: &'static str,
        ctx: &mut ViewContext<Self>,
    ) -> ViewHandle<EditorView> {
        let text = {
            let appearance = Appearance::as_ref(ctx);
            TextOptions::ui_text(Some(appearance.ui_font_size()), appearance)
        };
        ctx.add_view(|ctx| {
            let mut editor = EditorView::single_line(
                SingleLineEditorOptions {
                    text,
                    ..Default::default()
                },
                ctx,
            );
            editor.set_placeholder_text(placeholder, ctx);
            editor
        })
    }

    fn on_model_changed(&mut self, ctx: &mut ViewContext<Self>) {
        if let Some(id) = self.editing
            && self.model.as_ref(ctx).data().task(id).is_none()
        {
            self.editing = None;
        }
        self.sync_mouse_states(ctx);
        ctx.notify();
    }

    fn sync_mouse_states(&mut self, ctx: &AppContext) {
        let data = self.model.as_ref(ctx).data();
        self.task_mouse_states
            .retain(|id, _| data.task(*id).is_some());
        for id in data.ids() {
            self.task_mouse_states.entry(id).or_default();
        }
    }

    fn start_edit(&mut self, id: TaskId, ctx: &mut ViewContext<Self>) {
        let Some(text) = self
            .model
            .as_ref(ctx)
            .data()
            .task(id)
            .map(|task| task.text.clone())
        else {
            return;
        };
        self.editing = Some(id);
        self.edit_input
            .update(ctx, |editor, ctx| editor.set_buffer_text(&text, ctx));
        ctx.focus(&self.edit_input);
        ctx.notify();
    }

    fn cancel_edit(&mut self, ctx: &mut ViewContext<Self>) {
        if self.editing.take().is_some() {
            ctx.focus(&self.new_task_input);
            ctx.notify();
        }
    }

    fn open_menu(&mut self, id: TaskId, offset: Vector2F, ctx: &mut ViewContext<Self>) {
        let Some(task) = self.model.as_ref(ctx).data().task(id) else {
            return;
        };
        let item = |label: &str, action: NotesAction| {
            MenuItemFields::new(label)
                .with_on_select_action(action)
                .into_item()
        };
        let items = if task.archived {
            vec![
                item(
                    "Restore",
                    NotesAction::SetArchived {
                        id,
                        archived: false,
                    },
                ),
                MenuItem::Separator,
                item("Delete", NotesAction::Delete(id)),
            ]
        } else {
            vec![
                item(
                    if task.is_done() {
                        "Mark as not done"
                    } else {
                        "Mark as done"
                    },
                    NotesAction::ToggleDone(id),
                ),
                item("Edit", NotesAction::StartEdit(id)),
                item("Archive", NotesAction::SetArchived { id, archived: true }),
                MenuItem::Separator,
                item("Delete", NotesAction::Delete(id)),
            ]
        };
        self.context_menu.update(ctx, move |menu, ctx| {
            menu.set_items(items, ctx);
            ctx.notify();
        });
        self.menu_open = true;
        self.menu_offset = offset;
        ctx.focus(&self.context_menu);
        ctx.notify();
    }

    fn render_header(&self, appearance: &Appearance, app: &AppContext) -> Box<dyn Element> {
        let theme = appearance.theme();
        let data = self.model.as_ref(app).data();
        let (total, done) = data.active().fold((0, 0), |(total, done), task| {
            (total + 1, done + usize::from(task.is_done()))
        });
        let summary = if total == 0 {
            String::new()
        } else {
            format!("{done} of {total} done")
        };
        let mut row = Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(
                Text::new_inline(
                    "Today",
                    appearance.ui_font_family(),
                    appearance.ui_font_size(),
                )
                .with_color(theme.foreground().into())
                .with_style(Properties::default().weight(Weight::Bold))
                .finish(),
            )
            .with_child(
                Shrinkable::new(
                    1.0,
                    Container::new(dim_text(summary, appearance))
                        .with_margin_left(8.)
                        .finish(),
                )
                .finish(),
            )
            .with_child(Expanded::new(1.0, Empty::new().finish()).finish());
        if done > 0 {
            row = row.with_child(
                appearance
                    .ui_builder()
                    .button(ButtonVariant::Text, self.archive_done_mouse_state.clone())
                    .with_text_label("Archive done".into())
                    .with_style(UiComponentStyles {
                        height: Some(22.),
                        padding: Some(Coords {
                            left: 6.,
                            right: 6.,
                            ..Default::default()
                        }),
                        font_color: Some(theme.active_ui_text_color().into()),
                        ..Default::default()
                    })
                    .build()
                    .on_click(|ctx, _, _| ctx.dispatch_typed_action(NotesAction::ArchiveDone))
                    .finish(),
            );
        }
        Container::new(ConstrainedBox::new(row.finish()).with_height(24.).finish())
            .with_horizontal_padding(12.)
            .with_vertical_padding(6.)
            .finish()
    }

    fn render_input(&self, appearance: &Appearance) -> Box<dyn Element> {
        Container::new(text_input(self.new_task_input.clone(), appearance))
            .with_horizontal_padding(12.)
            .with_padding_bottom(8.)
            .finish()
    }

    fn render_body(&self, appearance: &Appearance, app: &AppContext) -> Box<dyn Element> {
        let theme = appearance.theme();
        let data = self.model.as_ref(app).data();
        let mut column = Flex::column();

        let mut active = data.active().peekable();
        if active.peek().is_none() {
            column = column.with_child(
                Container::new(dim_text(
                    "Nothing planned yet. Add a task above and press Enter.".into(),
                    appearance,
                ))
                .with_vertical_padding(4.)
                .finish(),
            );
        }
        for task in active {
            column = column.with_child(self.render_task(task, appearance));
        }

        let archived_count = data.archived_count();
        if archived_count > 0 {
            column = column.with_child(self.render_archived_header(archived_count, appearance));
            if self.show_archived {
                let today = Local::now().date_naive();
                for (day, tasks) in data.archived_by_day() {
                    column = column.with_child(
                        Container::new(dim_text(day_label(day, today), appearance))
                            .with_margin_top(6.)
                            .with_margin_bottom(2.)
                            .finish(),
                    );
                    for task in tasks {
                        column = column.with_child(self.render_task(task, appearance));
                    }
                }
            }
        }

        let content = Container::new(column.finish())
            .with_padding_left(12.)
            .with_padding_right(ScrollbarWidth::Auto.as_f32() + 12.)
            .with_padding_bottom(12.)
            .finish();
        ClippedScrollable::vertical(
            self.scroll_state.clone(),
            content,
            ScrollbarWidth::Auto,
            theme.nonactive_ui_detail().into(),
            theme.active_ui_detail().into(),
            Fill::None,
        )
        .with_overlayed_scrollbar()
        .finish()
    }

    fn render_archived_header(&self, count: usize, appearance: &Appearance) -> Box<dyn Element> {
        let theme = appearance.theme();
        let color = theme.sub_text_color(theme.background());
        let icon = if self.show_archived {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        };
        let label = format!("Archived ({count})");
        let font_family = appearance.ui_font_family();
        let font_size = appearance.ui_font_size();
        let header = Hoverable::new(self.archived_header_mouse_state.clone(), move |_| {
            Flex::row()
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_child(
                    ConstrainedBox::new(icon.to_warpui_icon(color).finish())
                        .with_width(12.)
                        .with_height(12.)
                        .finish(),
                )
                .with_child(
                    Container::new(
                        Text::new_inline(label.clone(), font_family, font_size)
                            .with_color(color.into())
                            .with_style(Properties::default().weight(Weight::Semibold))
                            .finish(),
                    )
                    .with_margin_left(4.)
                    .finish(),
                )
                .finish()
        })
        .with_cursor(Cursor::PointingHand)
        .on_click(|ctx, _, _| ctx.dispatch_typed_action(NotesAction::ToggleArchivedSection))
        .finish();
        Container::new(header).with_margin_top(14.).finish()
    }

    fn render_task(&self, task: &Task, appearance: &Appearance) -> Box<dyn Element> {
        let Some(states) = self.task_mouse_states.get(&task.id) else {
            return Empty::new().finish();
        };
        let theme = appearance.theme();
        let id = task.id;

        let marker = if task.archived {
            let color = theme.sub_text_color(theme.background());
            Container::new(
                ConstrainedBox::new(Icon::Check.to_warpui_icon(color).finish())
                    .with_width(CHECKBOX_SIZE)
                    .with_height(CHECKBOX_SIZE)
                    .finish(),
            )
            .with_margin_top(2.)
            .finish()
        } else {
            Container::new(
                appearance
                    .ui_builder()
                    .checkbox(states.checkbox.clone(), Some(CHECKBOX_SIZE))
                    .check(task.is_done())
                    .build()
                    .with_cursor(Cursor::PointingHand)
                    .on_click(move |ctx, _, _| {
                        ctx.dispatch_typed_action(NotesAction::ToggleDone(id))
                    })
                    .finish(),
            )
            .with_margin_top(1.)
            .finish()
        };

        let label = if self.editing == Some(id) {
            text_input(self.edit_input.clone(), appearance)
        } else {
            task_text(task, appearance)
        };

        let row = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Start)
            .with_child(marker)
            .with_child(
                Expanded::new(1.0, Container::new(label).with_margin_left(8.).finish()).finish(),
            )
            .finish();

        let hover = internal_colors::fg_overlay_2(theme);
        let position_id = self.position_id.clone();
        let archived = task.archived;
        let mut hoverable = Hoverable::new(states.row.clone(), move |mouse| {
            let mut container = Container::new(row)
                .with_horizontal_padding(6.)
                .with_vertical_padding(4.)
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)));
            if mouse.is_hovered() {
                container = container.with_background(hover);
            }
            container.finish()
        })
        .on_right_click(move |ctx, _, position| {
            let Some(bounds) = ctx.element_position_by_id(&position_id) else {
                return;
            };
            let offset = position - bounds.origin();
            ctx.dispatch_typed_action(NotesAction::OpenMenu {
                id,
                x: offset.x(),
                y: offset.y(),
            });
        });
        if !archived {
            hoverable = hoverable.on_double_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(NotesAction::StartEdit(id))
            });
        }
        Container::new(hoverable.finish())
            .with_margin_bottom(ROW_SPACING)
            .finish()
    }
}

fn task_text(task: &Task, appearance: &Appearance) -> Box<dyn Element> {
    let theme = appearance.theme();
    let mut text = Text::new(
        task.text.clone(),
        appearance.ui_font_family(),
        appearance.ui_font_size(),
    )
    .soft_wrap(true);
    if task.is_done() {
        let dim = theme.sub_text_color(theme.background()).into();
        text = text.with_color(dim).with_single_highlight(
            Highlight::new().with_text_style(TextStyle::new().with_show_strikethrough(true)),
            (0..task.text.chars().count()).collect(),
        );
    } else {
        text = text.with_color(theme.foreground().into());
    }
    text.finish()
}

fn text_input(editor: ViewHandle<EditorView>, appearance: &Appearance) -> Box<dyn Element> {
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
            ..Default::default()
        })
        .build()
        .finish()
}

fn dim_text(text: String, appearance: &Appearance) -> Box<dyn Element> {
    let theme = appearance.theme();
    Text::new(
        text,
        appearance.ui_font_family(),
        appearance.ui_font_size() - 1.,
    )
    .with_color(theme.sub_text_color(theme.background()).into())
    .soft_wrap(true)
    .finish()
}

fn day_label(day: Option<NaiveDate>, today: NaiveDate) -> String {
    match day {
        Some(day) if day == today => "Today".into(),
        Some(day) if day == today - Duration::days(1) => "Yesterday".into(),
        Some(day) => day.format("%a, %b %-d, %Y").to_string(),
        None => "Not finished".into(),
    }
}

impl Entity for NotesView {
    type Event = ();
}

impl View for NotesView {
    fn ui_name() -> &'static str {
        "NotesView"
    }

    fn on_focus(&mut self, focus_ctx: &FocusContext, ctx: &mut ViewContext<Self>) {
        if focus_ctx.is_self_focused() {
            ctx.focus(&self.new_task_input);
        }
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let mut column = Flex::column()
            .with_child(self.render_header(appearance, app))
            .with_child(self.render_input(appearance));
        if let Some(error) = self.model.as_ref(app).error() {
            column = column.with_child(
                Container::new(
                    Text::new(
                        error,
                        appearance.ui_font_family(),
                        appearance.ui_font_size() - 1.,
                    )
                    .with_color(appearance.theme().ui_error_color())
                    .soft_wrap(true)
                    .finish(),
                )
                .with_horizontal_padding(12.)
                .with_padding_bottom(8.)
                .finish(),
            );
        }
        column = column.with_child(Expanded::new(1.0, self.render_body(appearance, app)).finish());

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
        stack.finish()
    }
}

impl TypedActionView for NotesView {
    type Action = NotesAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        self.menu_open = false;
        match action {
            NotesAction::ToggleDone(id) => {
                self.model
                    .update(ctx, |model, ctx| model.toggle_done(*id, ctx));
            }
            NotesAction::OpenMenu { id, x, y } => self.open_menu(*id, vec2f(*x, *y), ctx),
            NotesAction::StartEdit(id) => self.start_edit(*id, ctx),
            NotesAction::Delete(id) => {
                self.model.update(ctx, |model, ctx| model.delete(*id, ctx));
            }
            NotesAction::SetArchived { id, archived } => {
                self.model
                    .update(ctx, |model, ctx| model.set_archived(*id, *archived, ctx));
            }
            NotesAction::ArchiveDone => {
                self.model.update(ctx, |model, ctx| model.archive_done(ctx));
            }
            NotesAction::ToggleArchivedSection => {
                self.show_archived = !self.show_archived;
            }
        }
        ctx.notify();
    }
}
