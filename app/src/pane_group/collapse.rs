//! Collapsed panes: a pane in a split can shrink to a thin strip (side-by-side splits) or bar
//! (stacked splits) in its own slot while its session keeps running; clicking the strip expands
//! it again.

use pathfinder_geometry::vector::vec2f;
use warp_core::ui::Icon;
use warp_core::ui::theme::color::internal_colors;
use warpui::elements::{
    Border, ChildAnchor, ConstrainedBox, Container, CrossAxisAlignment, Element, Flex, Hoverable,
    MainAxisAlignment, MainAxisSize, MouseStateHandle, OffsetPositioning, ParentAnchor,
    ParentElement, ParentOffsetBounds, SavePosition, Shrinkable, Stack, Text,
};
use warpui::platform::Cursor;
use warpui::text_layout::ClipConfig;
use warpui::ui_components::components::UiComponent as _;
use warpui::{AppContext, SingletonEntity as _};

use super::tree::SplitDirection;
use super::{PaneGroupAction, PaneId};
use crate::appearance::Appearance;
use crate::themes::theme::WarpTheme;

/// Width of a collapsed strip in a side-by-side split, and height of a collapsed bar in a
/// stacked one.
pub const COLLAPSED_PANE_SIZE: f32 = 28.0;
const ICON_SIZE: f32 = 14.0;

/// What a collapsed pane's strip shows.
pub struct CollapsedPane {
    pub title: String,
    pub mouse_state: MouseStateHandle,
}

/// The strip for a collapsed pane in a split along `axis`. `first` tells which way its expand
/// arrow points: a strip at the start of its split expands toward the end, any other toward the
/// start. Carries the pane's position id so geometry lookups (directional focus) still find it.
pub fn render_collapsed_pane(
    pane_id: PaneId,
    pane: &CollapsedPane,
    axis: SplitDirection,
    first: bool,
    theme: &WarpTheme,
    app: &AppContext,
) -> Box<dyn Element> {
    let appearance = Appearance::as_ref(app);
    let fg = theme.sub_text_color(theme.background());
    let hover_bg = internal_colors::fg_overlay_3(theme);
    let border = theme.outline();
    let icon = |icon: Icon| {
        ConstrainedBox::new(icon.to_warpui_icon(fg).finish())
            .with_width(ICON_SIZE)
            .with_height(ICON_SIZE)
            .finish()
    };
    let title = pane.title.clone();
    let font = appearance.ui_font_family();
    let font_size = appearance.ui_font_size() - 1.;
    let tooltip = appearance
        .ui_builder()
        .tool_tip(format!("Expand \"{title}\""))
        .build()
        .finish();

    let strip = Hoverable::new(pane.mouse_state.clone(), move |mouse| {
        let content: Box<dyn Element> = match axis {
            SplitDirection::Horizontal => {
                let arrow = if first {
                    Icon::ChevronRightDouble
                } else {
                    Icon::ChevronLeftDouble
                };
                ConstrainedBox::new(
                    Flex::column()
                        .with_main_axis_size(MainAxisSize::Max)
                        .with_cross_axis_alignment(CrossAxisAlignment::Center)
                        .with_child(Container::new(icon(arrow)).with_margin_top(8.).finish())
                        .with_child(
                            Container::new(icon(Icon::Terminal))
                                .with_margin_top(8.)
                                .finish(),
                        )
                        .finish(),
                )
                .with_width(COLLAPSED_PANE_SIZE)
                .finish()
            }
            SplitDirection::Vertical => {
                let arrow = if first {
                    Icon::ChevronDown
                } else {
                    Icon::ChevronUp
                };
                ConstrainedBox::new(
                    Flex::row()
                        .with_main_axis_size(MainAxisSize::Max)
                        .with_main_axis_alignment(MainAxisAlignment::Start)
                        .with_cross_axis_alignment(CrossAxisAlignment::Center)
                        .with_child(Container::new(icon(arrow)).with_margin_left(8.).finish())
                        .with_child(
                            Container::new(icon(Icon::Terminal))
                                .with_margin_left(6.)
                                .finish(),
                        )
                        .with_child(
                            Shrinkable::new(
                                1.0,
                                Container::new(
                                    Text::new_inline(title.clone(), font, font_size)
                                        .with_color(fg.into())
                                        .with_clip(ClipConfig::ellipsis())
                                        .finish(),
                                )
                                .with_margin_left(6.)
                                .with_margin_right(8.)
                                .finish(),
                            )
                            .finish(),
                        )
                        .finish(),
                )
                .with_height(COLLAPSED_PANE_SIZE)
                .finish()
            }
        };
        let mut container = Container::new(content).with_border(match axis {
            SplitDirection::Horizontal if first => Border::right(1.).with_border_fill(border),
            SplitDirection::Horizontal => Border::left(1.).with_border_fill(border),
            SplitDirection::Vertical if first => Border::bottom(1.).with_border_fill(border),
            SplitDirection::Vertical => Border::top(1.).with_border_fill(border),
        });
        if mouse.is_hovered() {
            container = container.with_background(hover_bg);
        }
        let mut stack = Stack::new().with_child(container.finish());
        // Strips have no room for a title, so it shows on hover; bars already show it.
        if mouse.is_hovered() && matches!(axis, SplitDirection::Horizontal) {
            let (parent, child, offset) = if first {
                (ParentAnchor::TopRight, ChildAnchor::TopLeft, vec2f(4., 4.))
            } else {
                (ParentAnchor::TopLeft, ChildAnchor::TopRight, vec2f(-4., 4.))
            };
            stack.add_positioned_overlay_child(
                tooltip,
                OffsetPositioning::offset_from_parent(
                    offset,
                    ParentOffsetBounds::WindowByPosition,
                    parent,
                    child,
                ),
            );
        }
        stack.finish()
    })
    .with_cursor(Cursor::PointingHand)
    .on_click(move |ctx, _, _| ctx.dispatch_typed_action(PaneGroupAction::ExpandPane(pane_id)))
    .finish();
    SavePosition::new(strip, &pane_id.position_id()).finish()
}
