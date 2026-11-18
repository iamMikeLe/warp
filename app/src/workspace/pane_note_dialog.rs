//! Modal dialog for adding or editing a pane's note (shown in the vertical tabs panel).

use pathfinder_geometry::vector::vec2f;
use warp_core::ui::theme::Fill;
use warpui::elements::{
    Align, ChildAnchor, ChildView, ConstrainedBox, Container, CornerRadius, Hoverable,
    MouseStateHandle, OffsetPositioning, ParentAnchor, ParentOffsetBounds, Radius, Stack, Text,
};
use warpui::fonts::Weight;
use warpui::keymap::FixedBinding;
use warpui::platform::Cursor;
use warpui::ui_components::button::ButtonVariant;
use warpui::ui_components::components::{Coords, UiComponent, UiComponentStyles};
use warpui::{
    AppContext, Element, Entity, SingletonEntity, TypedActionView, View, ViewContext, ViewHandle,
};

use crate::appearance::Appearance;
use crate::editor::{
    EditorOptions, EditorView, EnterAction, EnterSettings, Event as EditorEvent, TextOptions,
};
use crate::ui_components::dialog::{Dialog, dialog_styles};
use crate::workspace::PaneViewLocator;

pub fn init(app: &mut AppContext) {
    use warpui::keymap::macros::*;

    app.register_fixed_bindings([FixedBinding::new(
        "escape",
        PaneNoteDialogAction::Cancel,
        id!(PaneNoteDialog::ui_name()),
    )]);
}

const DIALOG_WIDTH: f32 = 460.;
const EDITOR_MIN_HEIGHT: f32 = 120.;
const EDITOR_MAX_HEIGHT: f32 = 280.;

pub struct PaneNoteDialog {
    editor: ViewHandle<EditorView>,
    cancel_mouse_state: MouseStateHandle,
    save_mouse_state: MouseStateHandle,
    /// The pane being edited; `None` until the dialog is first opened.
    locator: Option<PaneViewLocator>,
}

impl PaneNoteDialog {
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        let text = {
            let appearance = Appearance::as_ref(ctx);
            TextOptions::ui_text(Some(appearance.ui_font_size()), appearance)
        };
        let editor = ctx.add_view(|ctx| {
            let mut editor = EditorView::new(
                EditorOptions {
                    text,
                    soft_wrap: true,
                    autogrow: true,
                    enter_settings: EnterSettings {
                        enter: EnterAction::InsertNewLineIfMultiLine,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ctx,
            );
            editor.set_placeholder_text("Write a note for this pane", ctx);
            editor
        });
        ctx.subscribe_to_view(&editor, |me, _, event, ctx| match event {
            EditorEvent::CmdEnter => me.save(ctx),
            EditorEvent::Escape => ctx.emit(PaneNoteDialogEvent::Cancel),
            _ => {}
        });
        Self {
            editor,
            cancel_mouse_state: Default::default(),
            save_mouse_state: Default::default(),
            locator: None,
        }
    }

    /// Points the dialog at `locator` and fills the editor with its current note.
    pub fn open(&mut self, locator: PaneViewLocator, note: &str, ctx: &mut ViewContext<Self>) {
        self.locator = Some(locator);
        self.editor.update(ctx, |editor, ctx| {
            editor.set_buffer_text(note, ctx);
            editor.move_to_buffer_end(ctx);
        });
        ctx.notify();
    }

    fn save(&mut self, ctx: &mut ViewContext<Self>) {
        let Some(locator) = self.locator else {
            return;
        };
        let note = self.editor.as_ref(ctx).buffer_text(ctx);
        ctx.emit(PaneNoteDialogEvent::Save { locator, note });
    }
}

impl Entity for PaneNoteDialog {
    type Event = PaneNoteDialogEvent;
}

impl View for PaneNoteDialog {
    fn ui_name() -> &'static str {
        "PaneNoteDialog"
    }

    fn on_focus(&mut self, _focus_ctx: &warpui::FocusContext, ctx: &mut ViewContext<Self>) {
        ctx.focus(&self.editor);
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();

        let editor = Container::new(
            ConstrainedBox::new(ChildView::new(&self.editor).finish())
                .with_min_height(EDITOR_MIN_HEIGHT)
                .with_max_height(EDITOR_MAX_HEIGHT)
                .finish(),
        )
        .with_uniform_padding(8.)
        .with_border(warpui::elements::Border::all(1.).with_border_fill(theme.outline()))
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)))
        .finish();

        let save_button = appearance
            .ui_builder()
            .button(ButtonVariant::Accent, self.save_mouse_state.clone())
            .with_text_label("Save".to_string())
            .with_style(UiComponentStyles {
                font_size: Some(14.),
                font_weight: Some(Weight::Bold),
                height: Some(36.),
                padding: Some(Coords {
                    left: 16.,
                    right: 16.,
                    top: 0.,
                    bottom: 0.,
                }),
                ..Default::default()
            })
            .build()
            .with_cursor(Cursor::PointingHand)
            .on_click(|ctx, _, _| ctx.dispatch_typed_action(PaneNoteDialogAction::Save))
            .finish();

        let cancel_text_color = theme.sub_text_color(theme.surface_2());
        let cancel_button = Container::new(
            Hoverable::new(self.cancel_mouse_state.clone(), move |mouse_state| {
                let color = if mouse_state.is_mouse_over_element() {
                    theme.main_text_color(theme.surface_2())
                } else {
                    cancel_text_color
                };
                Text::new_inline("Cancel", appearance.ui_font_family(), 14.)
                    .with_color(color.into_solid())
                    .finish()
            })
            .with_cursor(Cursor::PointingHand)
            .on_click(|ctx, _, _| ctx.dispatch_typed_action(PaneNoteDialogAction::Cancel))
            .finish(),
        )
        .with_margin_right(16.)
        .finish();

        let dialog = Dialog::new(
            "Pane note".into(),
            Some(
                "Shown under the pane in the vertical tabs panel and when hovering it. \
                 Cmd/Ctrl+Enter saves."
                    .into(),
            ),
            UiComponentStyles {
                width: Some(DIALOG_WIDTH),
                padding: Some(Coords::uniform(24.)),
                ..dialog_styles(appearance)
            },
        )
        .with_child(editor)
        .with_bottom_row_child(cancel_button)
        .with_bottom_row_child(save_button)
        .build()
        .finish();

        let mut stack = Stack::new();
        stack.add_positioned_child(
            dialog,
            OffsetPositioning::offset_from_parent(
                vec2f(0., 0.),
                ParentOffsetBounds::WindowByPosition,
                ParentAnchor::Center,
                ChildAnchor::Center,
            ),
        );

        // Blurs the background and blocks interaction with it.
        Container::new(Align::new(stack.finish()).finish())
            .with_background_color(Fill::blur().into())
            .with_corner_radius(app.windows().window_corner_radius())
            .finish()
    }
}

pub enum PaneNoteDialogEvent {
    Save {
        locator: PaneViewLocator,
        note: String,
    },
    Cancel,
}

#[derive(Debug)]
pub enum PaneNoteDialogAction {
    Save,
    Cancel,
}

impl TypedActionView for PaneNoteDialog {
    type Action = PaneNoteDialogAction;

    fn handle_action(&mut self, action: &PaneNoteDialogAction, ctx: &mut ViewContext<Self>) {
        match action {
            PaneNoteDialogAction::Save => self.save(ctx),
            PaneNoteDialogAction::Cancel => ctx.emit(PaneNoteDialogEvent::Cancel),
        }
    }
}
