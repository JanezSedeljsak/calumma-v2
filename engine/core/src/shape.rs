use num_enum::{IntoPrimitive, TryFromPrimitive};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, IntoPrimitive, TryFromPrimitive)]
#[repr(u32)]
pub enum Tool {
    #[default]
    Pen = 0,
    Line = 1,
    Rect = 2,
    Ellipse = 3,
    Arrow = 4,
    Eraser = 5,
    SelectRect = 6,
    SelectEllipse = 7,
    SelectLasso = 8,
    Fill = 9,
    Transform = 10,
    Eyedropper = 11,
    Triangle = 12,
    Pentagon = 13,
    Text = 14,
    Move = 15,
    Blur = 16,
    MagicWand = 17,
    SelectColor = 18,
    Clone = 19,
    Heal = 20,
}

impl Tool {
    pub fn from_u32(v: u32) -> Option<Self> {
        Self::try_from(v).ok()
    }

    pub fn is_shape(self) -> bool {
        matches!(
            self,
            Tool::Line | Tool::Rect | Tool::Ellipse | Tool::Arrow | Tool::Triangle | Tool::Pentagon
        )
    }

    pub fn is_selection(self) -> bool {
        matches!(
            self,
            Tool::SelectRect
                | Tool::SelectEllipse
                | Tool::SelectLasso
                | Tool::MagicWand
                | Tool::SelectColor
        )
    }

    /// Whether this tool lays ink down with a brush. Only the pen: the eraser takes ink away
    /// and the shape tools fill an outline, neither of which has a brush's character.
    pub fn takes_brush(self) -> bool {
        matches!(self, Tool::Pen)
    }

    /// Whether this tool has an edge worth softening but no brush to carry it. The eraser
    /// alone: the pen's hardness rides in with its brush, and nothing else strokes a rim.
    pub fn takes_eraser_hardness(self) -> bool {
        matches!(self, Tool::Eraser)
    }

    /// Whether this tool floods from the pixel under the pointer, and so needs a tolerance.
    /// The select tools that answer by *reading* the layer rather than by describing a region.
    /// A layer with nothing painted has no answer for these two, while a marquee or a lasso is
    /// still a perfectly good region to draw on it — which is the whole difference the tool
    /// gate makes between them.
    pub fn samples_layer_pixels(self) -> bool {
        matches!(self, Tool::MagicWand | Tool::SelectColor)
    }

    /// One knob shared by the bucket and the wand, because they share one traversal — see
    /// `fill::flood_region`.
    pub fn takes_tolerance(self) -> bool {
        matches!(self, Tool::Fill | Tool::MagicWand | Tool::SelectColor)
    }

    /// Whether this tool encloses an area, and so can carry a fill and an outline
    /// independently. Line and Arrow are outlines with nothing inside them, so their ink is
    /// the one color they have always had.
    pub fn takes_fill(self) -> bool {
        matches!(
            self,
            Tool::Rect | Tool::Ellipse | Tool::Triangle | Tool::Pentagon
        )
    }

    pub fn takes_brush_size(self) -> bool {
        matches!(
            self,
            Tool::Pen
                | Tool::Line
                | Tool::Rect
                | Tool::Ellipse
                | Tool::Arrow
                | Tool::Eraser
                | Tool::Triangle
                | Tool::Pentagon
                | Tool::Blur
                | Tool::Clone
                | Tool::Heal
        )
    }

    /// Whether this tool reads the pixels already on the layer and writes a function of them,
    /// rather than writing a color over them. Blur is the first; sharpen and smudge would
    /// join it. Such a tool has no ink, so it takes neither color nor ink opacity.
    pub fn takes_blur_strength(self) -> bool {
        matches!(self, Tool::Blur)
    }

    /// Whether this tool samples pixels from elsewhere on the layer and so needs a source —
    /// `⌥`-click sets one, and the **Aligned** toggle decides whether it stays put relative to
    /// the brush across strokes (on) or snaps back to the anchor on the next one (off). Shared
    /// by both source-based retouching tools, since they read the same `CloneSource`.
    pub fn takes_clone_aligned(self) -> bool {
        matches!(self, Tool::Clone | Tool::Heal)
    }

    /// Whether this tool reads a color off the board, and so needs a sample area. A single
    /// pixel off an antialiased edge or a grainy brush is rarely the color the eye reads
    /// there, which is why the default averages — see `limits::EYEDROPPER_RADIUS_DEFAULT`.
    pub fn takes_eyedropper_radius(self) -> bool {
        matches!(self, Tool::Eyedropper)
    }

    /// Whether an in-progress stroke draws itself on the board as an ink preview. A blur has
    /// no color to preview and the GPU does not have the layer's source pixels to hand, so it
    /// commits into tiles as the pointer moves and the board shows the real result instead of
    /// a stand-in.
    pub fn previews_stroke(self) -> bool {
        matches!(self, Tool::Pen | Tool::Eraser | Tool::SelectLasso)
    }

    /// Whether a drag with this tool is a freehand stroke rather than a two-point shape drag.
    pub fn is_stroke(self) -> bool {
        matches!(
            self,
            Tool::Pen | Tool::Eraser | Tool::SelectLasso | Tool::Blur | Tool::Clone | Tool::Heal
        )
    }

    pub fn takes_ink_opacity(self) -> bool {
        matches!(
            self,
            Tool::Pen
                | Tool::Line
                | Tool::Rect
                | Tool::Ellipse
                | Tool::Arrow
                | Tool::Triangle
                | Tool::Pentagon
                | Tool::Fill
        )
    }

    pub fn shows_vector_mode(self) -> bool {
        self.is_shape() || self == Tool::Pen
    }

    /// Whether a Shift-drag squares this tool off: a rectangle becomes a square, an ellipse
    /// a circle. Line and Arrow would want an angle snap and the polygons a regular-polygon
    /// lock — different clamps, not this one — so they are deliberately not included.
    pub fn constrains_to_square(self) -> bool {
        matches!(self, Tool::Rect | Tool::Ellipse)
    }
}

/// Where a constrained drag from `start` really ends: the square that *fills* the drag. The
/// side is the longer of the two deltas, so the shape grows with the pointer instead of
/// collapsing to the shorter one, and each delta keeps its sign, so dragging up and to the
/// left still draws up and to the left.
pub fn square_end(start: (f32, f32), end: (f32, f32)) -> (f32, f32) {
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let side = dx.abs().max(dy.abs());
    (start.0 + side.copysign(dx), start.1 + side.copysign(dy))
}

const BARB_ANGLE: f32 = 0.5;
const HEAD_RATIO: f32 = 6.0;
const MIN_HEAD: f32 = 10.0;
const MAX_HEAD: f32 = 80.0;

/// Two endpoints and a style. `fill` and `stroke` are independent for the tools that
/// enclose an area (`Tool::takes_fill`): either, both, or — briefly, while the shell is
/// between toggles — neither. Line and Arrow ignore both and are always stroked, because an
/// outline is all they are.
///
/// Geometry only, no colors: the same struct answers where a *selection* rectangle is
/// (`selection.rs`), and it is what `board.wgsl` mirrors per pixel. Which color goes on
/// the fill and which on the stroke is the painter's business — `VectorShape` for a vector
/// item, the document's ink for a raster commit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub tool: Tool,
    pub start: (f32, f32),
    pub end: (f32, f32),
    pub half_width: f32,
    pub fill: bool,
    pub stroke: bool,
}

fn length(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}

pub fn sd_segment(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (pa_x, pa_y) = (p.0 - a.0, p.1 - a.1);
    let (ba_x, ba_y) = (b.0 - a.0, b.1 - a.1);
    let squared = ba_x * ba_x + ba_y * ba_y;
    let h = if squared > 0.0 {
        ((pa_x * ba_x + pa_y * ba_y) / squared).clamp(0.0, 1.0)
    } else {
        0.0
    };
    length(pa_x - ba_x * h, pa_y - ba_y * h)
}

fn sd_box(p: (f32, f32), center: (f32, f32), half: (f32, f32)) -> f32 {
    let dx = (p.0 - center.0).abs() - half.0;
    let dy = (p.1 - center.1).abs() - half.1;
    length(dx.max(0.0), dy.max(0.0)) + dx.max(dy).min(0.0)
}

fn sd_ellipse(p: (f32, f32), center: (f32, f32), radii: (f32, f32)) -> f32 {
    let rx = radii.0.max(f32::MIN_POSITIVE);
    let ry = radii.1.max(f32::MIN_POSITIVE);
    let (dx, dy) = (p.0 - center.0, p.1 - center.1);
    let outer = length(dx / rx, dy / ry);
    let gradient = length(dx / (rx * rx), dy / (ry * ry));
    if gradient <= f32::MIN_POSITIVE {
        return -rx.min(ry);
    }
    (outer - 1.0) * outer / gradient
}

pub fn sd_polygon(p: (f32, f32), verts: &[(f32, f32)]) -> f32 {
    let Some(&first) = verts.first() else {
        return f32::MAX;
    };
    let mut d = {
        let dx = p.0 - first.0;
        let dy = p.1 - first.1;
        dx * dx + dy * dy
    };
    let mut s = 1.0f32;
    let n = verts.len();
    for i in 0..n {
        let j = (i + 1) % n;
        let (e0, e1) = (verts[j].0 - verts[i].0, verts[j].1 - verts[i].1);
        let (w0, w1) = (p.0 - verts[i].0, p.1 - verts[i].1);
        let e_len2 = e0 * e0 + e1 * e1;
        let t = if e_len2 > 0.0 {
            ((w0 * e0 + w1 * e1) / e_len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (b0, b1) = (w0 - e0 * t, w1 - e1 * t);
        d = d.min(b0 * b0 + b1 * b1);

        let c0 = p.1 >= verts[i].1;
        let c1 = p.1 < verts[j].1;
        let c2 = e0 * w1 > e1 * w0;
        if (c0 && c1 && c2) || (!c0 && !c1 && !c2) {
            s = -s;
        }
    }
    s * d.sqrt()
}

fn triangle_verts(start: (f32, f32), end: (f32, f32)) -> [(f32, f32); 3] {
    let x0 = start.0.min(end.0);
    let y0 = start.1.min(end.1);
    let x1 = start.0.max(end.0);
    let y1 = start.1.max(end.1);
    [((x0 + x1) * 0.5, y0), (x1, y1), (x0, y1)]
}

fn pentagon_verts(start: (f32, f32), end: (f32, f32)) -> [(f32, f32); 5] {
    let center = ((start.0 + end.0) * 0.5, (start.1 + end.1) * 0.5);
    let rx = ((end.0 - start.0).abs() * 0.5).max(1e-3);
    let ry = ((end.1 - start.1).abs() * 0.5).max(1e-3);
    let mut verts = [(0.0, 0.0); 5];
    for (i, slot) in verts.iter_mut().enumerate() {
        let angle = -std::f32::consts::FRAC_PI_2 + (i as f32) * std::f32::consts::TAU / 5.0;
        *slot = (center.0 + angle.cos() * rx, center.1 + angle.sin() * ry);
    }
    verts
}

impl Shape {
    pub fn head_len(&self) -> f32 {
        (self.half_width * HEAD_RATIO).clamp(MIN_HEAD, MAX_HEAD)
    }

    pub fn triangle_vertices(&self) -> [(f32, f32); 3] {
        triangle_verts(self.start, self.end)
    }

    pub fn pentagon_vertices(&self) -> [(f32, f32); 5] {
        pentagon_verts(self.start, self.end)
    }

    /// Shaft plus the two barbs, as one open polyline — the outline an SVG `<polygon>`
    /// needs, derived from the same head geometry `arrow_distance` hit-tests against so the
    /// exported arrow matches the drawn one.
    pub fn arrow_outline(&self) -> Vec<(f32, f32)> {
        let (dx, dy) = (self.end.0 - self.start.0, self.end.1 - self.start.1);
        let span = length(dx, dy);
        if span <= f32::MIN_POSITIVE {
            return vec![self.start, self.end];
        }
        let head = self.head_len().min(span);
        let (ux, uy) = (-dx / span * head, -dy / span * head);
        let (sin, cos) = (BARB_ANGLE.sin(), BARB_ANGLE.cos());
        let left = (
            self.end.0 + ux * cos - uy * sin,
            self.end.1 + ux * sin + uy * cos,
        );
        let right = (
            self.end.0 + ux * cos + uy * sin,
            self.end.1 - ux * sin + uy * cos,
        );
        vec![self.start, self.end, left, self.end, right]
    }

    fn center(&self) -> (f32, f32) {
        (
            (self.start.0 + self.end.0) * 0.5,
            (self.start.1 + self.end.1) * 0.5,
        )
    }

    fn half_extent(&self) -> (f32, f32) {
        (
            (self.end.0 - self.start.0).abs() * 0.5,
            (self.end.1 - self.start.1).abs() * 0.5,
        )
    }

    fn arrow_distance(&self, p: (f32, f32)) -> f32 {
        let shaft = sd_segment(p, self.start, self.end);
        let (dx, dy) = (self.end.0 - self.start.0, self.end.1 - self.start.1);
        let span = length(dx, dy);
        if span <= f32::MIN_POSITIVE {
            return shaft;
        }
        let head = self.head_len().min(span);
        let (ux, uy) = (-dx / span * head, -dy / span * head);
        let (sin, cos) = (BARB_ANGLE.sin(), BARB_ANGLE.cos());
        let left = (
            self.end.0 + ux * cos - uy * sin,
            self.end.1 + ux * sin + uy * cos,
        );
        let right = (
            self.end.0 + ux * cos + uy * sin,
            self.end.1 - ux * sin + uy * cos,
        );
        shaft
            .min(sd_segment(p, self.end, left))
            .min(sd_segment(p, self.end, right))
    }

    /// The shape's region before any styling: negative inside the rectangle, ellipse or
    /// polygon; for Line and Arrow, the distance to the centreline, since they have no
    /// interior. `f32::MAX` for the tools that draw nothing.
    ///
    /// Everything the fill and the stroke need comes off this one number, which is why the
    /// SDF is evaluated once per pixel however many parts the shape has.
    pub fn region_distance(&self, x: f32, y: f32) -> f32 {
        let p = (x, y);
        match self.tool {
            Tool::Pen
            | Tool::Eraser
            | Tool::SelectRect
            | Tool::SelectEllipse
            | Tool::SelectLasso
            | Tool::Fill
            | Tool::Transform
            | Tool::Eyedropper
            | Tool::Text
            | Tool::Move
            | Tool::Blur
            | Tool::MagicWand
            | Tool::SelectColor
            | Tool::Clone
            | Tool::Heal => f32::MAX,
            Tool::Line => sd_segment(p, self.start, self.end),
            Tool::Arrow => self.arrow_distance(p),
            Tool::Rect => sd_box(p, self.center(), self.half_extent()),
            Tool::Ellipse => sd_ellipse(p, self.center(), self.half_extent()),
            Tool::Triangle => sd_polygon(p, &triangle_verts(self.start, self.end)),
            Tool::Pentagon => sd_polygon(p, &pentagon_verts(self.start, self.end)),
        }
    }

    /// Distance to the filled interior, or `None` when this shape has no fill.
    pub fn fill_distance(&self, x: f32, y: f32) -> Option<f32> {
        if !self.fill || !self.tool.takes_fill() {
            return None;
        }
        Some(self.region_distance(x, y))
    }

    /// Distance to the outline band — an annulus of `half_width` straddling the region's
    /// zero, or for Line and Arrow the stroked centreline itself.
    pub fn stroke_distance(&self, x: f32, y: f32) -> Option<f32> {
        let region = self.region_distance(x, y);
        if region == f32::MAX {
            return None;
        }
        if !self.tool.takes_fill() {
            return Some(region - self.half_width);
        }
        self.stroke.then(|| region.abs() - self.half_width)
    }

    /// The union of whatever parts this shape draws — what picking, bounds and hit-testing
    /// ask about, where "is any ink here" is the only question.
    pub fn distance(&self, x: f32, y: f32) -> f32 {
        match (self.fill_distance(x, y), self.stroke_distance(x, y)) {
            (Some(fill), Some(stroke)) => fill.min(stroke),
            (Some(d), None) | (None, Some(d)) => d,
            (None, None) => f32::MAX,
        }
    }

    pub fn coverage(&self, x: f32, y: f32) -> f32 {
        (0.5 - self.distance(x, y)).clamp(0.0, 1.0)
    }

    /// How far past its geometry this shape's ink reaches. A fill ends at the region's own
    /// edge, so a shape with no stroke pays only the antialiased pixel — the stroke width is
    /// what hangs off the outline, and an arrow's head off its end.
    pub fn padding(&self) -> f32 {
        let outline = if self.tool.takes_fill() && !self.stroke {
            0.0
        } else {
            self.half_width
        };
        let head = if self.tool == Tool::Arrow {
            self.head_len()
        } else {
            0.0
        };
        outline + head + 1.0
    }

    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        let pad = self.padding();
        (
            self.start.0.min(self.end.0) - pad,
            self.start.1.min(self.end.1) - pad,
            self.start.0.max(self.end.0) + pad,
            self.start.1.max(self.end.1) + pad,
        )
    }
}

/// Antialiased coverage from a signed distance: the same half-pixel band the shader uses,
/// so a flattened export matches what the board drew.
pub fn distance_coverage(distance: f32) -> f32 {
    (0.5 - distance).clamp(0.0, 1.0)
}

/// One part of a shape's paint as a straight-alpha source color, or `None` where it lays
/// nothing down. `distance` is `None` when the part is switched off entirely, which is what
/// lets a caller ask for fill and stroke in one expression and blend whatever comes back.
pub fn ink_sample(distance: Option<f32>, color: [u8; 4]) -> Option<[u8; 4]> {
    let coverage = distance_coverage(distance?);
    if coverage <= 0.0 {
        return None;
    }
    let mut rgba = color;
    rgba[3] = ((color[3] as f32) * coverage).round().clamp(0.0, 255.0) as u8;
    (rgba[3] != 0).then_some(rgba)
}
