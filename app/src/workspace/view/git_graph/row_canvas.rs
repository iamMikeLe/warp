//! Custom drawing element for a single row of commit-DAG lanes.
//!
//! Because of rendering-layer limitations (`Scene` only has rect / text / icon /
//! image primitives — no line / path / rotation), connections are drawn as
//! **orthogonal polylines**: vertical and horizontal segments are both thin
//! rectangles, and the commit dot is a square with `corner_radius = radius`.
//! Further beautification of rounded polyline corners is left for later polish.

use pathfinder_color::ColorU;
use pathfinder_geometry::rect::RectF;
use pathfinder_geometry::vector::{Vector2F, vec2f};
use warpui::elements::{Border, CornerRadius, Element, Point, Radius};
use warpui::event::DispatchedEvent;
use warpui::{AppContext, EventContext, LayoutContext, PaintContext, SizeConstraint};

use super::layout::GraphRow;

/// Pixel width of a single lane.
pub(crate) const LANE_WIDTH: f32 = 16.0;
/// Row height (aligned with the text rows on the right; UniformList uses this as
/// the uniform row height).
pub(crate) const ROW_HEIGHT: f32 = 28.0;
/// Line thickness.
const LINE_THICKNESS: f32 = 2.0;
/// Commit dot diameter.
const DOT_DIAMETER: f32 = 10.0;
/// The lane area never takes more than this share of the row, so the hash, refs,
/// and subject always stay visible however many lanes the loaded history needs.
/// Lanes past the cap are clipped.
const MAX_GRAPH_FRACTION: f32 = 0.4;

/// Lane color palette, indexed by `color_idx % LEN`. Phase 5 may replace this
/// with theme tokens.
/// Built with struct literals (`ColorU::new` is not a const fn and can't be used
/// in a const array).
const PALETTE: [ColorU; 7] = [
    ColorU {
        r: 0x4f,
        g: 0xc1,
        b: 0xff,
        a: 0xff,
    }, // blue
    ColorU {
        r: 0x4e,
        g: 0xc9,
        b: 0x7a,
        a: 0xff,
    }, // green
    ColorU {
        r: 0xff,
        g: 0xb0,
        b: 0x4f,
        a: 0xff,
    }, // orange
    ColorU {
        r: 0xd6,
        g: 0x7c,
        b: 0xff,
        a: 0xff,
    }, // purple
    ColorU {
        r: 0xff,
        g: 0x6e,
        b: 0x6e,
        a: 0xff,
    }, // red
    ColorU {
        r: 0x4f,
        g: 0xe0,
        b: 0xd6,
        a: 0xff,
    }, // cyan
    ColorU {
        r: 0xe6,
        g: 0xd2,
        b: 0x4f,
        a: 0xff,
    }, // yellow
];

pub(crate) fn lane_color(idx: usize) -> ColorU {
    PALETTE[idx % PALETTE.len()]
}

/// Element that draws one row of graph lanes. Its width is determined by the
/// number of lane columns, and its height is fixed at [`ROW_HEIGHT`].
pub(crate) struct GitGraphRowCanvas {
    row: GraphRow,
    /// The graph's maximum lane count, which determines this element's width (and
    /// thus the alignment of the whole column).
    lane_count: usize,
    /// Draw the node as a hollow ring instead of a filled dot — used only for
    /// the synthetic "uncommitted changes" row (which reads as "not a real
    /// commit"). The HEAD "you are here" marker is drawn next to the branch pill
    /// instead (see `render_ref_badge`), not on the lane.
    hollow: bool,
    /// Width set by the Graph column (see `columns.rs`); `None` sizes to the
    /// lane count. Either way the Phase 1 cap still applies.
    fixed_width: Option<f32>,
    /// When the commit carries a ref, a line in that ref's color from the left
    /// edge of the lane area to the node, continuing the connector drawn in the
    /// Branch / Tag column.
    ref_connector: Option<ColorU>,
    origin: Option<Point>,
    size: Option<Vector2F>,
}

impl GitGraphRowCanvas {
    pub(crate) fn new(row: GraphRow, lane_count: usize, hollow: bool) -> Self {
        Self {
            row,
            lane_count,
            hollow,
            fixed_width: None,
            ref_connector: None,
            origin: None,
            size: None,
        }
    }

    /// Sizes the lane area to the Graph column's width instead of the lane count.
    pub(crate) fn with_width(mut self, width: f32) -> Self {
        self.fixed_width = Some(width);
        self
    }

    /// Draws a connector from the left edge to the node (see [`Self::ref_connector`]).
    pub(crate) fn with_ref_connector(mut self, color: Option<ColorU>) -> Self {
        self.ref_connector = color;
        self
    }

    /// Width needed to draw every lane column.
    fn full_width(&self) -> f32 {
        self.lane_count.max(1) as f32 * LANE_WIDTH
    }

    /// Whether lane column `col` fits entirely inside a lane area `width` wide.
    fn col_visible(col: usize, width: f32) -> bool {
        (col + 1) as f32 * LANE_WIDTH <= width + 0.5
    }

    fn col_center_x(origin: Vector2F, col: usize) -> f32 {
        origin.x() + col as f32 * LANE_WIDTH + LANE_WIDTH / 2.0
    }

    /// Draw a vertical line segment at column position `x` (`y0..y1`).
    fn draw_vertical(ctx: &mut PaintContext, x: f32, y0: f32, y1: f32, color: ColorU) {
        let height = y1 - y0;
        if height <= 0.0 {
            return;
        }
        ctx.scene
            .draw_rect_with_hit_recording(RectF::new(
                vec2f(x - LINE_THICKNESS / 2.0, y0),
                vec2f(LINE_THICKNESS, height),
            ))
            .with_background(color);
    }

    /// Draw a horizontal line segment at `y` (connecting two column centers `x0`
    /// and `x1`). Each end extends by half the line width to fill in the
    /// right-angle corners.
    fn draw_horizontal(ctx: &mut PaintContext, x0: f32, x1: f32, y: f32, color: ColorU) {
        let (lo, hi) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
        let width = hi - lo + LINE_THICKNESS;
        ctx.scene
            .draw_rect_with_hit_recording(RectF::new(
                vec2f(lo - LINE_THICKNESS / 2.0, y - LINE_THICKNESS / 2.0),
                vec2f(width, LINE_THICKNESS),
            ))
            .with_background(color);
    }
}

impl Element for GitGraphRowCanvas {
    fn layout(
        &mut self,
        constraint: SizeConstraint,
        _ctx: &mut LayoutContext,
        _app: &AppContext,
    ) -> Vector2F {
        // Every row gets the same constraint (the list width), so the cap is the
        // same on every row and the lane columns stay aligned.
        let available = constraint.max.x();
        let cap = if available.is_finite() {
            (available * MAX_GRAPH_FRACTION).max(LANE_WIDTH)
        } else {
            f32::INFINITY
        };
        let wanted = self.fixed_width.unwrap_or_else(|| self.full_width());
        let size = vec2f(wanted.min(cap), ROW_HEIGHT);
        self.size = Some(size);
        size
    }

    fn after_layout(&mut self, _ctx: &mut warpui::AfterLayoutContext, _app: &AppContext) {}

    fn paint(&mut self, origin: Vector2F, ctx: &mut PaintContext, _app: &AppContext) {
        self.origin = Some(Point::from_vec2f(origin, ctx.scene.z_index()));

        let top = origin.y();
        let mid = origin.y() + ROW_HEIGHT / 2.0;
        let bot = origin.y() + ROW_HEIGHT;
        let node_x = Self::col_center_x(origin, self.row.node_col);
        let radius = DOT_DIAMETER / 2.0;
        // Lanes beyond the capped width (see `layout`) are not drawn, and
        // connectors are clamped to the right edge.
        let width = self.size.map_or_else(|| self.full_width(), |s| s.x());
        let right_edge = origin.x() + width;
        let visible = |col: usize| Self::col_visible(col, width);
        let node_visible = visible(self.row.node_col);
        let clamp = |x: f32| x.min(right_edge);

        // Ref connector first, so lanes and the node draw over it.
        if let Some(color) = self.ref_connector {
            let end = if node_visible { node_x } else { right_edge };
            Self::draw_horizontal(ctx, origin.x(), end, mid, ColorU { a: 0x99, ..color });
        }

        // Continuing lanes passing through this row: a full-height vertical line.
        for lane in self.row.passing.iter().filter(|lane| visible(lane.col)) {
            Self::draw_vertical(
                ctx,
                Self::col_center_x(origin, lane.col),
                top,
                bot,
                lane_color(lane.color_idx),
            );
        }

        // The node's own upper half (when it continues from the previous row).
        // A hollow ring has a transparent center, so the segment is trimmed to the
        // ring's top edge — otherwise it would show through the hollow center and
        // make the ring read as a filled dot.
        if self.row.node_continues_up && node_visible {
            let y1 = if self.hollow { mid - radius } else { mid };
            Self::draw_vertical(ctx, node_x, top, y1, lane_color(self.row.node_color));
        }

        // Child commits merging into this node from above: vertical (top -> mid,
        // at the child column) + horizontal (child column -> node column, at the
        // midline).
        for child in &self.row.from_children {
            let child_x = Self::col_center_x(origin, child.col);
            let color = lane_color(child.color_idx);
            if visible(child.col) {
                Self::draw_vertical(ctx, child_x, top, mid, color);
            }
            if visible(child.col) || node_visible {
                Self::draw_horizontal(ctx, clamp(child_x), clamp(node_x), mid, color);
            }
        }

        // This node connecting down to each parent: horizontal (node column ->
        // parent column, at the midline) + vertical (mid -> bottom, at the parent
        // column).
        for parent in &self.row.to_parents {
            let parent_x = Self::col_center_x(origin, parent.col);
            let color = lane_color(parent.color_idx);
            if parent.col != self.row.node_col {
                if visible(parent.col) || node_visible {
                    Self::draw_horizontal(ctx, clamp(node_x), clamp(parent_x), mid, color);
                }
                if visible(parent.col) {
                    Self::draw_vertical(ctx, parent_x, mid, bot, color);
                }
            } else if node_visible {
                // Straight-down lane at the node column: trim it to the ring's
                // bottom edge for a hollow node so it doesn't bleed through the
                // transparent center.
                let y0 = if self.hollow { mid + radius } else { mid };
                Self::draw_vertical(ctx, parent_x, y0, bot, color);
            }
        }

        // Commit dot (corner_radius = radius -> a circle). The synthetic
        // uncommitted row draws a ring (border only) instead — reads as "not a
        // real commit" — and its transparent center lets the row's
        // hover/selection highlight show through.
        if !node_visible {
            return;
        }
        let color = lane_color(self.row.node_color);
        let dot = ctx.scene.draw_rect_with_hit_recording(RectF::new(
            vec2f(node_x - radius, mid - radius),
            vec2f(DOT_DIAMETER, DOT_DIAMETER),
        ));
        dot.with_corner_radius(CornerRadius::with_all(Radius::Pixels(radius)));
        if self.hollow {
            dot.with_border(Border::all(LINE_THICKNESS).with_border_fill(color));
        } else {
            dot.with_background(color);
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
