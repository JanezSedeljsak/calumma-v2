struct Uniforms {
    pan: vec2<f32>,
    zoom: f32,
    dpr: f32,
    doc_size: vec2<f32>,
    viewport: vec2<f32>,
    time: f32,
    dark: f32,
    hover_rect: vec4<f32>,
    desk: vec4<f32>,
    grid: vec4<f32>,
    paper_border: vec4<f32>,
    hover_enabled: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
}

@group(0) @binding(0) var<uniform> u: Uniforms;

struct VsOut {
    @builtin(position) position: vec4<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VsOut {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var out: VsOut;
    out.position = vec4<f32>(pos[idx], 0.0, 1.0);
    return out;
}

const DESK_CELL: f32 = 26.0;
const DESK_LINE_W: f32 = 1.0;
const DESK_CROSS_ARM: f32 = 3.5;
const DESK_CROSS_LINE_W: f32 = 1.1;
const PAPER_BORDER_W: f32 = 2.0;

fn desk_pattern(screen: vec2<f32>) -> vec3<f32> {
    var rgb = u.desk.rgb;

    let cell_id = floor(screen / DESK_CELL);
    let line_local = screen - cell_id * DESK_CELL;
    let on_line = line_local.x < DESK_LINE_W || line_local.y < DESK_LINE_W;
    if on_line {
        rgb = mix(rgb, u.grid.rgb, u.grid.a * 0.4);
    }

    let nearest = round(screen / DESK_CELL) * DESK_CELL;
    let cross_local = screen - nearest;
    let on_cross = (abs(cross_local.x) < DESK_CROSS_LINE_W * 0.5 && abs(cross_local.y) < DESK_CROSS_ARM)
        || (abs(cross_local.y) < DESK_CROSS_LINE_W * 0.5 && abs(cross_local.x) < DESK_CROSS_ARM);
    if on_cross {
        rgb = mix(rgb, u.grid.rgb, u.grid.a);
    }

    return rgb;
}

@fragment
fn fs_paper(in: VsOut) -> @location(0) vec4<f32> {
    let screen = in.position.xy / max(u.dpr, 1.0);
    let xy = (screen - u.pan) / max(u.zoom, 1e-6);
    var rgb = desk_pattern(screen);

    let inside = xy.x >= 0.0 && xy.y >= 0.0 && xy.x < u.doc_size.x && xy.y < u.doc_size.y;
    let band = PAPER_BORDER_W / max(u.zoom, 1e-6);
    let in_band = xy.x >= -band && xy.y >= -band
        && xy.x <= u.doc_size.x + band && xy.y <= u.doc_size.y + band;
    if in_band && !inside {
        rgb = mix(rgb, u.paper_border.rgb, u.paper_border.a);
    }

    if u.hover_enabled > 0.5 {
        let r = u.hover_rect;
        let hover_inside = xy.x >= r.x && xy.y >= r.y && xy.x <= r.z && xy.y <= r.w;
        if hover_inside {
            let edge = min(min(xy.x - r.x, r.z - xy.x), min(xy.y - r.y, r.w - xy.y));
            let dash = floor((xy.x + xy.y + u.time * 40.0) / 8.0);
            let on = (i32(dash) & 1) == 0;
            if edge < 2.0 / max(u.zoom, 1e-6) && on {
                rgb = mix(rgb, vec3<f32>(0.24, 0.78, 0.84), 0.85);
            }
        }
    }

    return vec4<f32>(rgb, 1.0);
}

struct TileCamera {
    pan: vec2<f32>,
    zoom: f32,
    dpr: f32,
    viewport: vec2<f32>,
    _pad: vec2<f32>,
}

struct TilePlacement {
    origin: vec2<f32>,
    tile_size: f32,
    _pad: f32,
}

struct LayerXform {
    pivot: vec2<f32>,
    offset: vec2<f32>,
    scale: vec2<f32>,
    rotation: f32,
    _pad: f32,
}

@group(0) @binding(0) var<uniform> tu: TileCamera;
@group(0) @binding(1) var tile_tex: texture_2d<f32>;
@group(0) @binding(2) var tile_sampler: sampler;
@group(0) @binding(3) var<uniform> tp: TilePlacement;
@group(0) @binding(4) var<uniform> lx: LayerXform;

struct TileVsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_tile(@builtin(vertex_index) idx: u32) -> TileVsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    let uv = corners[idx];
    let raw_doc = tp.origin + uv * tp.tile_size;
    let rel = (raw_doc - lx.pivot) * lx.scale;
    let s = sin(lx.rotation);
    let c = cos(lx.rotation);
    let rotated = vec2<f32>(rel.x * c - rel.y * s, rel.x * s + rel.y * c);
    let doc = lx.pivot + rotated + lx.offset;
    let screen = doc * tu.zoom + tu.pan;
    let device = screen * tu.dpr;
    let ndc = vec2<f32>(
        (device.x / max(tu.viewport.x, 1.0)) * 2.0 - 1.0,
        1.0 - (device.y / max(tu.viewport.y, 1.0)) * 2.0,
    );
    var out: TileVsOut;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_tile(input: TileVsOut) -> @location(0) vec4<f32> {
    let c = textureSampleLevel(tile_tex, tile_sampler, input.uv, 0.0);
    return vec4<f32>(c.rgb * c.a, c.a);
}

const TOOL_PEN: u32 = 0u;
const TOOL_LINE: u32 = 1u;
const TOOL_RECT: u32 = 2u;
const TOOL_ELLIPSE: u32 = 3u;
const TOOL_ARROW: u32 = 4u;
const TOOL_ERASER: u32 = 5u;
const TOOL_TRIANGLE: u32 = 12u;
const TOOL_PENTAGON: u32 = 13u;

const FILL_OUTLINE: f32 = 0.0;
const FILL_SOLID: f32 = 1.0;
const TAU: f32 = 6.28318530718;
const FRAC_PI_2: f32 = 1.57079632679;

fn is_filled(fill: f32) -> bool {
    return fill > 0.5;
}

fn sd_segment_pts(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = select(0.0, clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0), dot(ba, ba) > 0.0);
    return length(pa - ba * h);
}

fn sd_polygon3(p: vec2<f32>, v0: vec2<f32>, v1: vec2<f32>, v2: vec2<f32>) -> f32 {
    var d = dot(p - v0, p - v0);
    var s = 1.0;
    let e0 = v1 - v0;
    let w0 = p - v0;
    let b0 = w0 - e0 * select(0.0, clamp(dot(w0, e0) / dot(e0, e0), 0.0, 1.0), dot(e0, e0) > 0.0);
    d = min(d, dot(b0, b0));
    let c00 = p.y >= v0.y;
    let c01 = p.y < v1.y;
    let c02 = e0.x * w0.y > e0.y * w0.x;
    if (c00 && c01 && c02) || (!c00 && !c01 && !c02) { s = -s; }

    let e1 = v2 - v1;
    let w1 = p - v1;
    let b1 = w1 - e1 * select(0.0, clamp(dot(w1, e1) / dot(e1, e1), 0.0, 1.0), dot(e1, e1) > 0.0);
    d = min(d, dot(b1, b1));
    let c10 = p.y >= v1.y;
    let c11 = p.y < v2.y;
    let c12 = e1.x * w1.y > e1.y * w1.x;
    if (c10 && c11 && c12) || (!c10 && !c11 && !c12) { s = -s; }

    let e2 = v0 - v2;
    let w2 = p - v2;
    let b2 = w2 - e2 * select(0.0, clamp(dot(w2, e2) / dot(e2, e2), 0.0, 1.0), dot(e2, e2) > 0.0);
    d = min(d, dot(b2, b2));
    let c20 = p.y >= v2.y;
    let c21 = p.y < v0.y;
    let c22 = e2.x * w2.y > e2.y * w2.x;
    if (c20 && c21 && c22) || (!c20 && !c21 && !c22) { s = -s; }

    return s * sqrt(d);
}

fn sd_polygon5(p: vec2<f32>, v: array<vec2<f32>, 5>) -> f32 {
    var d = dot(p - v[0], p - v[0]);
    var s = 1.0;
    for (var i = 0u; i < 5u; i = i + 1u) {
        let j = (i + 1u) % 5u;
        let e = v[j] - v[i];
        let w = p - v[i];
        let b = w - e * select(0.0, clamp(dot(w, e) / dot(e, e), 0.0, 1.0), dot(e, e) > 0.0);
        d = min(d, dot(b, b));
        let c0 = p.y >= v[i].y;
        let c1 = p.y < v[j].y;
        let c2 = e.x * w.y > e.y * w.x;
        if (c0 && c1 && c2) || (!c0 && !c1 && !c2) {
            s = -s;
        }
    }
    return s * sqrt(d);
}

fn shape_distance(tool: u32, p0: vec2<f32>, p1: vec2<f32>, half_width: f32, fill: f32, p: vec2<f32>) -> f32 {
    let pa = p - p0;
    let ba = p1 - p0;
    let baba = dot(ba, ba);
    let h = select(0.0, clamp(dot(pa, ba) / baba, 0.0, 1.0), baba > 0.0);
    let seg = length(pa - ba * h);

    switch tool {
        case TOOL_LINE: {
            return seg - half_width;
        }
        case TOOL_ARROW: {
            let span = length(ba);
            var d = seg;
            if span > 1e-5 {
                let head = clamp(half_width * 6.0, 10.0, 80.0);
                let hl = min(head, span);
                let ux = -ba.x / span * hl;
                let uy = -ba.y / span * hl;
                let ang = 0.5;
                let s = sin(ang);
                let c = cos(ang);
                let left = p1 + vec2<f32>(ux * c - uy * s, ux * s + uy * c);
                let right = p1 + vec2<f32>(ux * c + uy * s, -ux * s + uy * c);
                d = min(d, sd_segment_pts(p, p1, left));
                d = min(d, sd_segment_pts(p, p1, right));
            }
            return d - half_width;
        }
        case TOOL_RECT: {
            let center = (p0 + p1) * 0.5;
            let half = abs(p1 - p0) * 0.5;
            let d = p - center;
            let q = abs(d) - half;
            let boxd = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0);
            if is_filled(fill) {
                return boxd;
            }
            return abs(boxd) - half_width;
        }
        case TOOL_ELLIPSE: {
            let center = (p0 + p1) * 0.5;
            let half = abs(p1 - p0) * 0.5;
            let d = p - center;
            let rx = max(half.x, 1e-5);
            let ry = max(half.y, 1e-5);
            let outer = length(vec2<f32>(d.x / rx, d.y / ry));
            let grad = length(vec2<f32>(d.x / (rx * rx), d.y / (ry * ry)));
            var ed = -min(rx, ry);
            if grad > 1e-5 {
                ed = (outer - 1.0) * outer / grad;
            }
            if is_filled(fill) {
                return ed;
            }
            return abs(ed) - half_width;
        }
        case TOOL_TRIANGLE: {
            let x0 = min(p0.x, p1.x);
            let y0 = min(p0.y, p1.y);
            let x1 = max(p0.x, p1.x);
            let y1 = max(p0.y, p1.y);
            let v0 = vec2<f32>((x0 + x1) * 0.5, y0);
            let v1 = vec2<f32>(x1, y1);
            let v2 = vec2<f32>(x0, y1);
            let d = sd_polygon3(p, v0, v1, v2);
            if is_filled(fill) {
                return d;
            }
            return abs(d) - half_width;
        }
        case TOOL_PENTAGON: {
            let center = (p0 + p1) * 0.5;
            let rx = max(abs(p1.x - p0.x) * 0.5, 1e-3);
            let ry = max(abs(p1.y - p0.y) * 0.5, 1e-3);
            var verts: array<vec2<f32>, 5>;
            for (var i = 0u; i < 5u; i = i + 1u) {
                let angle = -FRAC_PI_2 + f32(i) * TAU / 5.0;
                verts[i] = center + vec2<f32>(cos(angle) * rx, sin(angle) * ry);
            }
            let d = sd_polygon5(p, verts);
            if is_filled(fill) {
                return d;
            }
            return abs(d) - half_width;
        }
        case TOOL_PEN, default: {
            return 1e9;
        }
    }
}

struct PreviewUniforms {
    pan: vec2<f32>,
    zoom: f32,
    dpr: f32,
    viewport: vec2<f32>,
    _align_color: vec2<f32>,
    color: vec4<f32>,
    p0: vec2<f32>,
    p1: vec2<f32>,
    half_width: f32,
    tool: f32,
    fill: f32,
    _pad: f32,
}

@group(0) @binding(0) var<uniform> pu: PreviewUniforms;

struct StrokeIn {
    @location(0) segment: vec4<f32>,
    @location(1) color: vec4<f32>,
    @location(2) radius: f32,
}

struct StrokeOut {
    @builtin(position) position: vec4<f32>,
    @location(0) doc: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) p0: vec2<f32>,
    @location(3) p1: vec2<f32>,
    @location(4) radius: f32,
}

@vertex
fn vs_stroke(input: StrokeIn, @builtin(vertex_index) idx: u32) -> StrokeOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    let p0 = input.segment.xy;
    let p1 = input.segment.zw;
    let pad = vec2<f32>(input.radius + 1.0);
    let lo = min(p0, p1) - pad;
    let hi = max(p0, p1) + pad;
    let doc = mix(lo, hi, corners[idx]);
    let screen = doc * pu.zoom + pu.pan;
    let device = screen * pu.dpr;
    let ndc = vec2<f32>(
        (device.x / max(pu.viewport.x, 1.0)) * 2.0 - 1.0,
        1.0 - (device.y / max(pu.viewport.y, 1.0)) * 2.0,
    );
    var out: StrokeOut;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.doc = doc;
    out.color = input.color;
    out.p0 = p0;
    out.p1 = p1;
    out.radius = input.radius;
    return out;
}

@fragment
fn fs_stroke(input: StrokeOut) -> @location(0) vec4<f32> {
    let pa = input.doc - input.p0;
    let ba = input.p1 - input.p0;
    let baba = dot(ba, ba);
    let h = select(0.0, clamp(dot(pa, ba) / baba, 0.0, 1.0), baba > 0.0);
    let d = length(pa - ba * h) - input.radius;
    let cov = clamp(0.5 - d, 0.0, 1.0);
    if cov <= 0.0 {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(input.color.rgb, input.color.a * cov);
}

@vertex
fn vs_shape_preview(@builtin(vertex_index) idx: u32) -> VsOut {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var out: VsOut;
    out.position = vec4<f32>(pos[idx], 0.0, 1.0);
    return out;
}

@fragment
fn fs_shape_preview(in: VsOut) -> @location(0) vec4<f32> {
    let screen = in.position.xy / max(pu.dpr, 1.0);
    let xy = (screen - pu.pan) / max(pu.zoom, 1e-6);
    let tool = u32(pu.tool + 0.5);
    let d = shape_distance(tool, pu.p0, pu.p1, pu.half_width, pu.fill, xy);
    let cov = clamp(0.5 - d, 0.0, 1.0);
    if cov <= 0.0 {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(pu.color.rgb, pu.color.a * cov);
}
