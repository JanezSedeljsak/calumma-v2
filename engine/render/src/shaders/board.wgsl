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

const DESK_CELL: f32 = 39.0;
const DESK_LINE_W: f32 = 1.0;
const PAPER_BORDER_W: f32 = 2.0;

fn desk_pattern(screen: vec2<f32>) -> vec3<f32> {
    let cell_id = floor(screen / DESK_CELL);
    let local = screen - cell_id * DESK_CELL;
    let on_line = local.x < DESK_LINE_W || local.y < DESK_LINE_W;
    if on_line {
        return mix(u.desk.rgb, u.grid.rgb, u.grid.a);
    }
    return u.desk.rgb;
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

@group(0) @binding(0) var<uniform> tu: TileCamera;
@group(0) @binding(1) var tile_tex: texture_2d<f32>;
@group(0) @binding(2) var tile_sampler: sampler;
@group(0) @binding(3) var<uniform> tp: TilePlacement;

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
    let doc = tp.origin + uv * tp.tile_size;
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
    return textureSampleLevel(tile_tex, tile_sampler, input.uv, 0.0);
}

const TOOL_PEN: u32 = 0u;
const TOOL_LINE: u32 = 1u;
const TOOL_RECT: u32 = 2u;
const TOOL_ELLIPSE: u32 = 3u;
const TOOL_ARROW: u32 = 4u;

const FILL_OUTLINE: f32 = 0.0;
const FILL_SOLID: f32 = 1.0;

fn is_filled(fill: f32) -> bool {
    return fill > 0.5;
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
                let pa_l = p - p1;
                let ba_l = left - p1;
                let h_l = select(0.0, clamp(dot(pa_l, ba_l) / dot(ba_l, ba_l), 0.0, 1.0), dot(ba_l, ba_l) > 0.0);
                let pa_r = p - p1;
                let ba_r = right - p1;
                let h_r = select(0.0, clamp(dot(pa_r, ba_r) / dot(ba_r, ba_r), 0.0, 1.0), dot(ba_r, ba_r) > 0.0);
                d = min(d, length(pa_l - ba_l * h_l));
                d = min(d, length(pa_r - ba_r * h_r));
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
