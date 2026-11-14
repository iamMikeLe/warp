//! The flowchart's connector column for one row: guide lines for ancestors, an elbow from the
//! parent, and the row's node dot (drawn with rects, like the Git Graph's lanes).

use pathfinder_color::ColorU;
use pathfinder_geometry::rect::RectF;
use pathfinder_geometry::vector::{Vector2F, vec2f};
use warpui::elements::{Border, CornerRadius, Element, Point, Radius};
use warpui::event::DispatchedEvent;
use warpui::{AppContext, EventContext, LayoutContext, PaintContext, SizeConstraint};

use super::tree::TreeRow;

/// Width of one tree level.
pub(super) const COLUMN_WIDTH: f32 = 18.0;
const LINE_THICKNESS: f32 = 2.0;
const DOT_DIAMETER: f32 = 10.0;

pub(super) struct TreeGutter {
    row: TreeRow,
    height: f32,
    /// Vertical position of the node (aligned with the row's title line).
    node_y: f32,
    line_color: ColorU,
    dot_color: ColorU,
    /// A ring instead of a filled dot (branch nodes, which aren't worktrees).
    hollow: bool,
    origin: Option<Point>,
    size: Option<Vector2F>,
}

impl TreeGutter {
    pub(super) fn new(row: TreeRow, height: f32, node_y: f32) -> Self {
        Self {
            row,
            height,
            node_y,
            line_color: ColorU::new(0x80, 0x80, 0x80, 0xff),
            dot_color: ColorU::new(0x80, 0x80, 0x80, 0xff),
            hollow: false,
            origin: None,
            size: None,
        }
    }

    pub(super) fn with_colors(mut self, line: ColorU, dot: ColorU) -> Self {
        self.line_color = line;
        self.dot_color = dot;
        self
    }

    pub(super) fn hollow(mut self) -> Self {
        self.hollow = true;
        self
    }

    pub(super) fn width_for(depth: usize) -> f32 {
        (depth + 1) as f32 * COLUMN_WIDTH
    }

    fn center(origin: Vector2F, column: usize) -> f32 {
        origin.x() + column as f32 * COLUMN_WIDTH + COLUMN_WIDTH / 2.0
    }

    fn vertical(ctx: &mut PaintContext, x: f32, y0: f32, y1: f32, color: ColorU) {
        if y1 <= y0 {
            return;
        }
        ctx.scene
            .draw_rect_with_hit_recording(RectF::new(
                vec2f(x - LINE_THICKNESS / 2.0, y0),
                vec2f(LINE_THICKNESS, y1 - y0),
            ))
            .with_background(color);
    }

    fn horizontal(ctx: &mut PaintContext, x0: f32, x1: f32, y: f32, color: ColorU) {
        ctx.scene
            .draw_rect_with_hit_recording(RectF::new(
                vec2f(x0 - LINE_THICKNESS / 2.0, y - LINE_THICKNESS / 2.0),
                vec2f(x1 - x0 + LINE_THICKNESS, LINE_THICKNESS),
            ))
            .with_background(color);
    }
}

impl Element for TreeGutter {
    fn layout(
        &mut self,
        _constraint: SizeConstraint,
        _ctx: &mut LayoutContext,
        _app: &AppContext,
    ) -> Vector2F {
        let size = vec2f(Self::width_for(self.row.depth), self.height);
        self.size = Some(size);
        size
    }

    fn after_layout(&mut self, _ctx: &mut warpui::AfterLayoutContext, _app: &AppContext) {}

    fn paint(&mut self, origin: Vector2F, ctx: &mut PaintContext, _app: &AppContext) {
        self.origin = Some(Point::from_vec2f(origin, ctx.scene.z_index()));
        let top = origin.y();
        let bottom = origin.y() + self.height;
        let node_y = origin.y() + self.node_y;
        let color = self.line_color;
        let depth = self.row.depth;

        for (column, continues) in self.row.ancestors_continue.iter().enumerate() {
            if *continues {
                Self::vertical(ctx, Self::center(origin, column), top, bottom, color);
            }
        }
        let node_x = Self::center(origin, depth);
        if depth > 0 {
            let parent_x = Self::center(origin, depth - 1);
            let elbow_end = if self.row.is_last { node_y } else { bottom };
            Self::vertical(ctx, parent_x, top, elbow_end, color);
            Self::horizontal(ctx, parent_x, node_x, node_y, color);
        }
        if self.row.has_children {
            Self::vertical(ctx, node_x, node_y, bottom, color);
        }

        let radius = DOT_DIAMETER / 2.0;
        let dot = ctx.scene.draw_rect_with_hit_recording(RectF::new(
            vec2f(node_x - radius, node_y - radius),
            vec2f(DOT_DIAMETER, DOT_DIAMETER),
        ));
        dot.with_corner_radius(CornerRadius::with_all(Radius::Pixels(radius)));
        if self.hollow {
            dot.with_border(Border::all(LINE_THICKNESS).with_border_fill(self.dot_color));
        } else {
            dot.with_background(self.dot_color);
        }
    }

    fn size(&self) -> Option<Vector2F> {
        self.size
    }

    fn origin(&self) -> Option<Point> {
        self.origin
    }

    fn dispatch_event(
        &mut self,
        _event: &DispatchedEvent,
        _ctx: &mut EventContext,
        _app: &AppContext,
    ) -> bool {
        false
    }
}
