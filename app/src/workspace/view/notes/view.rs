//! The Notes panel: today's running todo list, with finished tasks moved to a collapsible
//! archive grouped by the day they were done. Selecting a task opens a details area below the
//! list with its title and a rich-text (markdown) description.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{Duration, Local, NaiveDate};
use warp_core::ui::Icon;
use warp_core::ui::theme::color::internal_colors;
use warp_editor::render::element::VerticalExpansionBehavior;
use warpui::elements::{
    Align, Border, ChildAnchor, ChildView, ClippedScrollStateHandle, ClippedScrollable,
    ConstrainedBox, Container, CornerRadius, CrossAxisAlignment, DEFAULT_UI_LINE_HEIGHT_RATIO,
    DragBarSide, Element, Empty, Expanded, Fill, Flex, Highlight, Hoverable, MainAxisSize,
    MouseStateHandle, OffsetPositioning, ParentElement, PositionedElementAnchor,
    PositionedElementOffsetBounds, Radius, Resizable, ResizableStateHandle, SavePosition,
    ScrollbarWidth, Shrinkable, Stack, Text, resizable_state_handle,
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
use crate::editor::{
    EditorOptions, EditorView, EnterAction, EnterSettings, Event as EditorEvent,
    SingleLineEditorOptions, TextOptions,
};
use crate::menu::{Menu, MenuItem, MenuItemFields};
use crate::notebooks::editor::model::NotebooksEditorModel;
use crate::notebooks::editor::rich_text_styles;
use crate::notebooks::editor::view::{EditorViewEvent, RichTextEditorConfig, RichTextEditorView};
use crate::notebooks::link::{NotebookLinks, SessionSource};
use crate::settings::FontSettings;
use crate::ui_components::buttons::icon_button;

const CONTEXT_MENU_WIDTH: f32 = 180.0;
const CHECKBOX_SIZE: f32 = 14.0;
const ROW_SPACING: f32 = 2.0;
const DETAIL_MIN_HEIGHT: f32 = 120.0;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum NotesAction {
    ToggleDone(TaskId),
    /// Opens the task's details, or closes them if it is already selected.
    Select(TaskId),
    /// Opens the task's details with the cursor in its title.
    EditTitle(TaskId),
    /// Shows the description as plain markdown (`true`) or formatted.
    SetRawDescription(bool),
    CloseDetails,
    OpenMenu {
        id: TaskId,
        x: f32,
        y: f32,
    },
    Delete(TaskId),
    SetArchived {
        id: TaskId,
        archived: bool,
    },
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
    title_input: ViewHandle<EditorView>,
    description_editor: ViewHandle<RichTextEditorView>,
    raw_description_editor: ViewHandle<EditorView>,
    show_raw_description: bool,
    selected: Option<TaskId>,
    /// The stored description last loaded into or saved from the editor; a different stored
    /// value means another window changed it.
    loaded_description: String,
    /// The editor's own markdown at that point, which can differ from the stored text in
    /// formatting, so reloading content doesn't count as an edit.
    editor_markdown: String,
    task_mouse_states: HashMap<TaskId, TaskMouseStates>,
    archive_done_mouse_state: MouseStateHandle,
    archived_header_mouse_state: MouseStateHandle,
    close_details_mouse_state: MouseStateHandle,
    formatted_mouse_state: MouseStateHandle,
    raw_mouse_state: MouseStateHandle,
    show_archived: bool,
    scroll_state: ClippedScrollStateHandle,
    detail_resizable_state: ResizableStateHandle,
    detail_height_initialized: Arc<AtomicBool>,
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

        let new_task_input = Self::single_line_editor("Add a task…", None, ctx);
        ctx.subscribe_to_view(&new_task_input, |me, editor, event, ctx| match event {
            EditorEvent::Enter => {
                let title = editor.as_ref(ctx).buffer_text(ctx);
                me.model.update(ctx, |model, ctx| model.add(&title, ctx));
                editor.update(ctx, |editor, ctx| editor.set_buffer_text("", ctx));
            }
            EditorEvent::Escape => {
                editor.update(ctx, |editor, ctx| editor.set_buffer_text("", ctx));
            }
            _ => {}
        });

        let title_input = Self::single_line_editor("Title", Some(Weight::Bold), ctx);
        ctx.subscribe_to_view(&title_input, |me, editor, event, ctx| match event {
            EditorEvent::Edited(_) => {
                if let Some(id) = me.selected {
                    let title = editor.as_ref(ctx).buffer_text(ctx);
                    me.model
                        .update(ctx, |model, ctx| model.set_title(id, &title, ctx));
                }
            }
            EditorEvent::Enter => me.focus_description(ctx),
            EditorEvent::Escape => me.close_details(ctx),
            _ => {}
        });

        let description_editor = Self::description_editor(ctx);
        ctx.subscribe_to_view(&description_editor, |me, _, event, ctx| match event {
            EditorViewEvent::Edited => me.save_description(ctx),
            EditorViewEvent::EscapePressed => ctx.focus(&me.new_task_input),
            _ => {}
        });

        let raw_description_editor = Self::raw_description_editor(ctx);
        ctx.subscribe_to_view(&raw_description_editor, |me, _, event, ctx| match event {
            EditorEvent::Edited(_) => me.save_raw_description(ctx),
            EditorEvent::Escape => ctx.focus(&me.new_task_input),
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
            title_input,
            description_editor,
            raw_description_editor,
            show_raw_description: false,
            selected: None,
            loaded_description: String::new(),
            editor_markdown: String::new(),
            task_mouse_states: HashMap::new(),
            archive_done_mouse_state: MouseStateHandle::default(),
            archived_header_mouse_state: MouseStateHandle::default(),
            close_details_mouse_state: MouseStateHandle::default(),
            formatted_mouse_state: MouseStateHandle::default(),
            raw_mouse_state: MouseStateHandle::default(),
            show_archived: false,
            scroll_state: ClippedScrollStateHandle::new(),
            detail_resizable_state: resizable_state_handle(DETAIL_MIN_HEIGHT),
            detail_height_initialized: Arc::new(AtomicBool::new(false)),
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
        weight: Option<Weight>,
        ctx: &mut ViewContext<Self>,
    ) -> ViewHandle<EditorView> {
        let mut text = {
            let appearance = Appearance::as_ref(ctx);
            TextOptions::ui_text(Some(appearance.ui_font_size()), appearance)
        };
        if let Some(weight) = weight {
            text.font_properties_override = Some(Properties::default().weight(weight));
        }
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

    /// The notebook editor (headings, lists, code blocks, the "/" block menu), with running
    /// commands and embedding Warp Drive objects turned off.
    fn description_editor(ctx: &mut ViewContext<Self>) -> ViewHandle<RichTextEditorView> {
        let styles = rich_text_styles(Appearance::as_ref(ctx), FontSettings::as_ref(ctx));
        let window_id = ctx.window_id();
        let parent_position_id = format!("NotesView_{}", ctx.view_id());
        let model = ctx.add_model(|ctx| NotebooksEditorModel::new(styles, window_id, ctx));
        let links = ctx.add_model(|ctx| NotebookLinks::new(SessionSource::Active(window_id), ctx));
        ctx.add_typed_action_view(|ctx| {
            RichTextEditorView::new(
                parent_position_id,
                model,
                links,
                RichTextEditorConfig {
                    gutter_width: Some(0.0),
                    embedded_objects_enabled: Some(false),
                    vertical_expansion_behavior: Some(VerticalExpansionBehavior::FillMaxHeight),
                    can_execute_shell_commands: Some(false),
                    ..Default::default()
                },
                ctx,
            )
        })
    }

    /// A plain monospace editor showing the description's markdown as stored.
    fn raw_description_editor(ctx: &mut ViewContext<Self>) -> ViewHandle<EditorView> {
        let mut text = {
            let appearance = Appearance::as_ref(ctx);
            TextOptions::ui_text(Some(appearance.monospace_font_size()), appearance)
        };
        text.font_family_override = Some(Appearance::as_ref(ctx).monospace_font_family());
        ctx.add_view(|ctx| {
            let mut editor = EditorView::new(
                EditorOptions {
                    text,
                    soft_wrap: true,
                    enter_settings: EnterSettings {
                        enter: EnterAction::InsertNewLineIfMultiLine,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ctx,
            );
            editor.set_placeholder_text("Markdown", ctx);
            editor
        })
    }

    fn on_model_changed(&mut self, ctx: &mut ViewContext<Self>) {
        self.sync_mouse_states(ctx);
        if let Some(id) = self.selected {
            match self.model.as_ref(ctx).data().task(id).cloned() {
                None => self.close_details(ctx),
                Some(task) => {
                    let title_input_text = self.title_input.as_ref(ctx).buffer_text(ctx);
                    if task.title != title_input_text.trim() {
                        self.title_input
                            .update(ctx, |editor, ctx| editor.set_buffer_text(&task.title, ctx));
                    }
                    if task.description != self.loaded_description {
                        self.load_description(&task.description, ctx);
                    }
                }
            }
        }
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

    fn select(&mut self, id: TaskId, ctx: &mut ViewContext<Self>) {
        let Some(task) = self.model.as_ref(ctx).data().task(id).cloned() else {
            return;
        };
        self.selected = Some(id);
        self.title_input
            .update(ctx, |editor, ctx| editor.set_buffer_text(&task.title, ctx));
        self.load_description(&task.description, ctx);
        ctx.notify();
    }

    /// Loads `description` into whichever editor is showing.
    fn load_description(&mut self, description: &str, ctx: &mut ViewContext<Self>) {
        self.loaded_description = description.to_string();
        if self.show_raw_description {
            self.raw_description_editor
                .update(ctx, |editor, ctx| editor.set_buffer_text(description, ctx));
            return;
        }
        let model = self.description_editor.as_ref(ctx).model().clone();
        model.update(ctx, |model, ctx| {
            model.reset_with_markdown(description, ctx)
        });
        self.editor_markdown = model.as_ref(ctx).markdown(ctx);
    }

    fn set_raw_description(&mut self, raw: bool, ctx: &mut ViewContext<Self>) {
        if self.show_raw_description == raw {
            return;
        }
        self.show_raw_description = raw;
        let Some(task) = self
            .selected
            .and_then(|id| self.model.as_ref(ctx).data().task(id).cloned())
        else {
            return;
        };
        self.load_description(&task.description, ctx);
        self.focus_description(ctx);
    }

    fn focus_description(&mut self, ctx: &mut ViewContext<Self>) {
        if self.show_raw_description {
            ctx.focus(&self.raw_description_editor);
        } else {
            self.description_editor
                .update(ctx, |editor, ctx| editor.focus(ctx));
        }
    }

    fn save_raw_description(&mut self, ctx: &mut ViewContext<Self>) {
        let Some(id) = self.selected else {
            return;
        };
        let markdown = self.raw_description_editor.as_ref(ctx).buffer_text(ctx);
        if markdown == self.loaded_description {
            return;
        }
        self.loaded_description = markdown.clone();
        self.model
            .update(ctx, |model, ctx| model.set_description(id, &markdown, ctx));
    }

    fn save_description(&mut self, ctx: &mut ViewContext<Self>) {
        let Some(id) = self.selected else {
            return;
        };
        let markdown = self
            .description_editor
            .as_ref(ctx)
            .model()
            .as_ref(ctx)
            .markdown(ctx);
        if markdown == self.editor_markdown {
            return;
        }
        self.editor_markdown = markdown.clone();
        self.loaded_description = markdown.clone();
        self.model
            .update(ctx, |model, ctx| model.set_description(id, &markdown, ctx));
    }

    fn close_details(&mut self, ctx: &mut ViewContext<Self>) {
        if self.selected.take().is_some() {
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
                item("Edit", NotesAction::EditTitle(id)),
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

    fn render_list(&self, appearance: &Appearance, app: &AppContext) -> Box<dyn Element> {
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
        let dim = theme.sub_text_color(theme.background());

        let marker = if task.archived {
            ConstrainedBox::new(Icon::Check.to_warpui_icon(dim).finish())
                .with_width(CHECKBOX_SIZE)
                .with_height(CHECKBOX_SIZE)
                .finish()
        } else {
            appearance
                .ui_builder()
                .checkbox(states.checkbox.clone(), Some(CHECKBOX_SIZE))
                .with_style(UiComponentStyles {
                    margin: Some(Coords::uniform(0.)),
                    ..Default::default()
                })
                .check(task.is_done())
                .build()
                .with_cursor(Cursor::PointingHand)
                .on_click(move |ctx, _, _| ctx.dispatch_typed_action(NotesAction::ToggleDone(id)))
                .finish()
        };
        // Centered on the first line of the title, so it lines up however the title wraps.
        let first_line_height = appearance.ui_font_size() * DEFAULT_UI_LINE_HEIGHT_RATIO;
        let marker = ConstrainedBox::new(Align::new(marker).finish())
            .with_width(CHECKBOX_SIZE)
            .with_height(first_line_height)
            .finish();

        let mut row = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Start)
            .with_child(marker)
            .with_child(
                Expanded::new(
                    1.0,
                    Container::new(task_title(task, appearance))
                        .with_margin_left(8.)
                        .finish(),
                )
                .finish(),
            );
        if !task.description.trim().is_empty() {
            row = row.with_child(
                Container::new(
                    ConstrainedBox::new(Icon::File.to_warpui_icon(dim).finish())
                        .with_width(12.)
                        .with_height(first_line_height)
                        .finish(),
                )
                .with_margin_left(6.)
                .finish(),
            );
        }
        let row = row.finish();

        let selected = self.selected == Some(id);
        let hover = internal_colors::fg_overlay_2(theme);
        let selected_background = internal_colors::fg_overlay_3(theme);
        let position_id = self.position_id.clone();
        let hoverable = Hoverable::new(states.row.clone(), move |mouse| {
            let mut container = Container::new(row)
                .with_horizontal_padding(6.)
                .with_vertical_padding(4.)
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)));
            if selected {
                container = container.with_background(selected_background);
            } else if mouse.is_hovered() {
                container = container.with_background(hover);
            }
            container.finish()
        })
        // Ticking the checkbox shouldn't also open the task.
        .with_defer_events_to_children()
        .with_cursor(Cursor::PointingHand)
        .on_click(move |ctx, _, _| ctx.dispatch_typed_action(NotesAction::Select(id)))
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
        Container::new(hoverable.finish())
            .with_margin_bottom(ROW_SPACING)
            .finish()
    }

    /// "Formatted | Raw": how the description is shown and edited.
    fn render_mode_switch(&self, appearance: &Appearance) -> Box<dyn Element> {
        let theme = appearance.theme();
        let segment = |label: &str, raw: bool, state: MouseStateHandle| {
            let active = self.show_raw_description == raw;
            let font_color = if active {
                theme.active_ui_text_color()
            } else {
                theme.nonactive_ui_text_color()
            };
            appearance
                .ui_builder()
                .button(ButtonVariant::Text, state)
                .with_text_label(label.to_string())
                .with_style(UiComponentStyles {
                    height: Some(22.),
                    padding: Some(Coords {
                        left: 6.,
                        right: 6.,
                        ..Default::default()
                    }),
                    font_color: Some(font_color.into()),
                    background: active.then(|| internal_colors::fg_overlay_3(theme).into()),
                    border_radius: Some(CornerRadius::with_all(Radius::Pixels(4.))),
                    ..Default::default()
                })
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(NotesAction::SetRawDescription(raw))
                })
                .finish()
        };
        Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(2.)
            .with_child(segment(
                "Formatted",
                false,
                self.formatted_mouse_state.clone(),
            ))
            .with_child(segment("Raw", true, self.raw_mouse_state.clone()))
            .finish()
    }

    /// The details area under the list; drag its top edge to resize it.
    fn render_details(&self, appearance: &Appearance) -> Box<dyn Element> {
        let theme = appearance.theme();
        let close = icon_button(
            appearance,
            Icon::X,
            false,
            self.close_details_mouse_state.clone(),
        )
        .build()
        .on_click(|ctx, _, _| ctx.dispatch_typed_action(NotesAction::CloseDetails))
        .finish();
        let title_row = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(
                Expanded::new(1.0, text_input(self.title_input.clone(), appearance)).finish(),
            )
            .with_child(
                Container::new(self.render_mode_switch(appearance))
                    .with_margin_left(6.)
                    .finish(),
            )
            .with_child(Container::new(close).with_margin_left(2.).finish())
            .finish();
        let description = if self.show_raw_description {
            ChildView::new(&self.raw_description_editor).finish()
        } else {
            ChildView::new(&self.description_editor).finish()
        };
        let details = Flex::column()
            .with_child(
                Container::new(title_row)
                    .with_horizontal_padding(12.)
                    .with_padding_top(10.)
                    .with_padding_bottom(6.)
                    .finish(),
            )
            .with_child(
                Expanded::new(
                    1.0,
                    Container::new(description)
                        .with_horizontal_padding(12.)
                        .with_padding_bottom(8.)
                        .finish(),
                )
                .finish(),
            )
            .finish();
        let details = Container::new(details)
            .with_border(Border::top(1.).with_border_fill(theme.outline()))
            .finish();

        let state = self.detail_resizable_state.clone();
        let initialized = self.detail_height_initialized.clone();
        Resizable::new(self.detail_resizable_state.clone(), details)
            .with_dragbar_side(DragBarSide::Top)
            .on_resize(|ctx, _| ctx.notify())
            .with_bounds_callback(Box::new(move |window_size| {
                let max = (window_size.y() * 0.8).max(DETAIL_MIN_HEIGHT);
                // The first time the details appear, give them half the window; afterwards keep
                // the height the user dragged to.
                if !initialized.swap(true, Ordering::Relaxed)
                    && let Ok(mut state) = state.lock()
                {
                    state.set_size((window_size.y() / 2.0).clamp(DETAIL_MIN_HEIGHT, max));
                }
                (DETAIL_MIN_HEIGHT, max)
            }))
            .finish()
    }
}

fn task_title(task: &Task, appearance: &Appearance) -> Box<dyn Element> {
    let theme = appearance.theme();
    let mut text = Text::new(
        task.title.clone(),
        appearance.ui_font_family(),
        appearance.ui_font_size(),
    )
    .soft_wrap(true);
    if task.is_done() {
        let dim = theme.sub_text_color(theme.background()).into();
        text = text.with_color(dim).with_single_highlight(
            Highlight::new().with_text_style(TextStyle::new().with_show_strikethrough(true)),
            (0..task.title.chars().count()).collect(),
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
        column = column.with_child(Expanded::new(1.0, self.render_list(appearance, app)).finish());
        if self.selected.is_some() {
            column = column.with_child(self.render_details(appearance));
        }

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
            NotesAction::Select(id) => {
                if self.selected == Some(*id) {
                    self.close_details(ctx);
                } else {
                    self.select(*id, ctx);
                    self.focus_description(ctx);
                }
            }
            NotesAction::EditTitle(id) => {
                self.select(*id, ctx);
                ctx.focus(&self.title_input);
            }
            NotesAction::SetRawDescription(raw) => self.set_raw_description(*raw, ctx),
            NotesAction::CloseDetails => self.close_details(ctx),
            NotesAction::OpenMenu { id, x, y } => self.open_menu(*id, vec2f(*x, *y), ctx),
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
