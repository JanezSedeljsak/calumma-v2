struct Uniforms {
    pan: vec2<f32>,
    zoom: f32,
    dpr: f32,
    doc_size: vec2<f32>,
    viewport: vec2<f32>,
    dark: f32,
    // Side of one period of the baked desk lattice, in device texels, or 0 where the backing
    // scale does not tile one — see `render/src/desk.rs`. Zero puts `desk_pattern` back on the
    // procedural path this replaced.
    lattice_side: f32,
    _pad1: f32,
    _pad2: f32,
    // (cell, line width, cross arm, cross line width) — `calumma_core::DeskMetrics`, so the
    // squared paper the shell's loading placeholder draws lands on the same lattice.
    desk_metrics: vec4<f32>,
    desk: vec4<f32>,
    grid: vec4<f32>,
    paper_border: vec4<f32>,
}

@group(0) @binding(0) var<uniform> u: Uniforms;
// One period of the desk grid: red is "on a cell rule", green is "on a corner cross". Read with
// `textureLoad` and an integer modulo rather than a sampler, so the texel a device pixel lands
// on is exact arithmetic at any viewport coordinate instead of a UV that has to survive being
// scaled up and wrapped back down.
@group(0) @binding(1) var desk_lattice: texture_2d<f32>;

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

const PAPER_BORDER_W: f32 = 2.0;
const DESK_LINE_ALPHA: f32 = 0.4;

// The two halves of the pattern as coverage, in the order they blend: cell rules at
// `DESK_LINE_ALPHA` of the grid color, corner crosses at full strength.
fn desk_lattice_coverage(screen: vec2<f32>, device: vec2<f32>) -> vec2<f32> {
    if u.lattice_side > 0.0 {
        let side = i32(u.lattice_side);
        let texel = vec2<i32>(device) % vec2<i32>(side, side);
        return textureLoad(desk_lattice, texel, 0).rg;
    }
    return desk_pattern_coverage(screen);
}

fn desk_pattern_coverage(screen: vec2<f32>) -> vec2<f32> {
    let cell = max(u.desk_metrics.x, 1.0);
    let line_w = u.desk_metrics.y;
    let cross_arm = u.desk_metrics.z;
    let cross_line_w = u.desk_metrics.w;

    let cell_id = floor(screen / cell);
    let line_local = screen - cell_id * cell;
    let on_line = line_local.x < line_w || line_local.y < line_w;

    let nearest = round(screen / cell) * cell;
    let cross_local = screen - nearest;
    let on_cross = (abs(cross_local.x) < cross_line_w * 0.5 && abs(cross_local.y) < cross_arm)
        || (abs(cross_local.y) < cross_line_w * 0.5 && abs(cross_local.x) < cross_arm);

    return vec2<f32>(f32(on_line), f32(on_cross));
}

fn desk_pattern(screen: vec2<f32>, device: vec2<f32>) -> vec3<f32> {
    let coverage = desk_lattice_coverage(screen, device);
    var rgb = u.desk.rgb;
    rgb = mix(rgb, u.grid.rgb, u.grid.a * DESK_LINE_ALPHA * coverage.x);
    rgb = mix(rgb, u.grid.rgb, u.grid.a * coverage.y);
    return rgb;
}

@fragment
fn fs_paper(in: VsOut) -> @location(0) vec4<f32> {
    let screen = in.position.xy / max(u.dpr, 1.0);
    let xy = (screen - u.pan) / max(u.zoom, 1e-6);
    var rgb = desk_pattern(screen, in.position.xy);

    let inside = xy.x >= 0.0 && xy.y >= 0.0 && xy.x < u.doc_size.x && xy.y < u.doc_size.y;
    let band = PAPER_BORDER_W / max(u.zoom, 1e-6);
    let in_band = xy.x >= -band && xy.y >= -band
        && xy.x <= u.doc_size.x + band && xy.y <= u.doc_size.y + band;
    if in_band && !inside {
        rgb = mix(rgb, u.paper_border.rgb, u.paper_border.a);
    }

    return vec4<f32>(rgb, 1.0);
}

struct TileCamera {
    pan: vec2<f32>,
    zoom: f32,
    dpr: f32,
    viewport: vec2<f32>,
    doc_size: vec2<f32>,
    crisp: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
}

// One document layer's contribution to a tile draw, indexed by the instance's `layer_index`.
// Row *i* is `doc.layers[i]` — stack position, so the index a tile instance carries needs no
// side table to resolve and a vector layer simply owns an unread row.
//
// `atlas_slot` is only read by the solid-Paper quad, which has no instance buffer to carry it
// the way `vs_tile` does. It used to ride in `pivot.x` as a `bitcast` union with this same
// struct; an explicit field costs 4 bytes a layer and stops two draw paths disagreeing about
// what the bytes mean.
//
// `opacity`, `lut_mode`, `tone`, `saturation` and `vibrance` are plan 23's addition: a layer's
// non-destructive adjustments, evaluated by `apply_adjustments` in `fs_tile` instead of baked
// into tile bytes by the CPU (`compose::composited_tile_payload`, mask only now). Nothing here
// is per *tile*: the whole point is that the table is written once per content rebuild — or
// once per slider sample, which no longer touches a tile at all — not once per draw.
struct LayerData {
    pivot: vec2<f32>,
    offset: vec2<f32>,
    scale: vec2<f32>,
    rotation: f32,
    opacity: f32,
    atlas_slot: u32,
    // LUT_MODE_IDENTITY / LUT_MODE_TONE / LUT_MODE_TONE_HSL below.
    lut_mode: u32,
    // `AdjustmentLut::tone` (Rust) mirrored byte for byte: gamma -> contrast -> brightness,
    // indexed by the input channel's own byte value, the same table for every channel.
    tone: array<f32, 256>,
    saturation: f32,
    vibrance: f32,
}

const LUT_MODE_IDENTITY: u32 = 0u;
const LUT_MODE_TONE: u32 = 1u;
const LUT_MODE_TONE_HSL: u32 = 2u;

// Every tile GPU-resident across the whole document lives in one shared array texture,
// addressed per-instance by array-layer index — see `TileAtlas` in `render/src/tile_atlas.rs`.
// That is what turns a document layer's tiles into a single instanced draw instead of one
// draw call per tile. Group 0 carries the atlas, the per-frame camera and the layer table, and
// is bound once for the whole board: there is no per-layer bind group, so a stack of Normal
// layers is one `set_bind_group` and one instanced draw per layer rather than a rebind each.
@group(0) @binding(0) var<uniform> tu: TileCamera;
@group(0) @binding(1) var tile_tex: texture_2d_array<f32>;
@group(0) @binding(2) var tile_sampler: sampler;
@group(0) @binding(3) var tile_sampler_crisp: sampler;
// Fragment-visible as of plan 23: `fs_tile` reads `tone`/`saturation`/`vibrance` here, not just
// `vs_tile` reading the transform. See `tile_shared_bgl` in `renderer.rs` for the binding.
@group(0) @binding(4) var<storage, read> layer_data: array<LayerData>;

// Must match `calumma_core::tile::TILE_SIZE` — tiles are square and fixed-size at runtime, so
// this is a shader constant rather than a per-instance value.
const TILE_SIZE_PX: f32 = 256.0;

struct TileInstanceIn {
    @location(0) origin: vec2<f32>,
    @location(1) slot: u32,
    @location(2) layer_index: u32,
}

struct TileVsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) slot: u32,
    // Carried through so `fs_tile` can read this instance's own `LayerData` row for opacity and
    // the adjustment LUT — `vs_tile` already reads it for the transform, but never passed it on.
    @location(2) @interpolate(flat) layer_index: u32,
}

@vertex
fn vs_tile(input: TileInstanceIn, @builtin(vertex_index) idx: u32) -> TileVsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    let uv = corners[idx];
    let lx = layer_data[input.layer_index];
    let raw_doc = input.origin + uv * TILE_SIZE_PX;
    var doc: vec2<f32>;
    if abs(lx.rotation) < 1e-6 && all(lx.scale == vec2<f32>(1.0, 1.0)) && all(lx.offset == vec2<f32>(0.0, 0.0)) {
        doc = raw_doc;
    } else if abs(lx.rotation) < 1e-6 {
        doc = lx.pivot + (raw_doc - lx.pivot) * lx.scale + lx.offset;
    } else {
        let rel = (raw_doc - lx.pivot) * lx.scale;
        let s = sin(lx.rotation);
        let c = cos(lx.rotation);
        let rotated = vec2<f32>(rel.x * c - rel.y * s, rel.x * s + rel.y * c);
        doc = lx.pivot + rotated + lx.offset;
    }
    let screen = doc * tu.zoom + tu.pan;
    let device = screen * tu.dpr;
    let ndc = vec2<f32>(
        (device.x / max(tu.viewport.x, 1.0)) * 2.0 - 1.0,
        1.0 - (device.y / max(tu.viewport.y, 1.0)) * 2.0,
    );
    var out: TileVsOut;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = uv;
    out.slot = input.slot;
    out.layer_index = input.layer_index;
    return out;
}

// The tile atlas is `Rgba8UnormSrgb` (`TileAtlas::create_array_texture`) and the swapchain
// prefers an sRGB format too (`Renderer::from_surface`), so `textureSample` below has already
// decoded the stored byte to linear light, and blending against an sRGB target happens in that
// same linear space — correct for compositing, but not the space `AdjustmentLut` (`core/src/
// filters.rs`) was built in: it runs directly on the stored byte, with no gamma curve at all.
// `apply_adjustments` re-encodes to that byte space before the lookup and decodes the result
// back, so a slider reads the same numbers flatten/export do. The two curves are software here
// and hardware on write, so they need not be bit-identical — `layer_table_tests` allows a
// one-code tolerance for exactly that reason.
fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        return c * 12.92;
    }
    return 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        return c / 12.92;
    }
    return pow((c + 0.055) / 1.055, 2.4);
}

// Mirrors `hsl_stage` in `core/src/filters.rs` byte for byte in spirit, not literally — Rust
// works in `[f32; 3]`, this in `vec3<f32>` — but every arithmetic step is the same. See that
// function's own comment for why saturation and vibrance share one HSL round trip.
fn hsl_stage(v: vec3<f32>, saturation: f32, vibrance: f32) -> vec3<f32> {
    if saturation == 0.0 && vibrance == 0.0 {
        return v;
    }
    var hsl = rgb_to_hsl(v);
    if saturation != 0.0 {
        hsl.y = clamp(hsl.y * (1.0 + saturation), 0.0, 1.0);
    }
    if vibrance != 0.0 {
        hsl.y = clamp(hsl.y + vibrance * (1.0 - hsl.y), 0.0, 1.0);
    }
    return hsl_to_rgb(hsl);
}

fn rgb_to_hsl(rgb: vec3<f32>) -> vec3<f32> {
    let mx = max(rgb.r, max(rgb.g, rgb.b));
    let mn = min(rgb.r, min(rgb.g, rgb.b));
    let l = (mx + mn) * 0.5;
    if abs(mx - mn) < 1e-6 {
        return vec3<f32>(0.0, 0.0, l);
    }
    let d = mx - mn;
    var s: f32;
    if l > 0.5 {
        s = d / (2.0 - mx - mn);
    } else {
        s = d / (mx + mn);
    }
    var h: f32;
    if mx == rgb.r {
        h = (rgb.g - rgb.b) / d % 6.0;
    } else if mx == rgb.g {
        h = (rgb.b - rgb.r) / d + 2.0;
    } else {
        h = (rgb.r - rgb.g) / d + 4.0;
    }
    // `fract` is exactly Rust's `.rem_euclid(1.0)` for a divide-by-one wrap: both are the
    // floor-based remainder that stays in `[0, 1)` regardless of `h`'s sign.
    return vec3<f32>(fract(h / 6.0), s, l);
}

fn hue_to_rgb(p: f32, q: f32, t_in: f32) -> f32 {
    var t = t_in;
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        return p + (q - p) * 6.0 * t;
    }
    if t < 0.5 {
        return q;
    }
    if t < 2.0 / 3.0 {
        return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    }
    return p;
}

fn hsl_to_rgb(hsl: vec3<f32>) -> vec3<f32> {
    let h = hsl.x;
    let s = hsl.y;
    let l = hsl.z;
    if s <= 1e-6 {
        return vec3<f32>(l, l, l);
    }
    var q: f32;
    if l < 0.5 {
        q = l * (1.0 + s);
    } else {
        q = l + s - l * s;
    }
    let p = 2.0 * l - q;
    return vec3<f32>(
        hue_to_rgb(p, q, h + 1.0 / 3.0),
        hue_to_rgb(p, q, h),
        hue_to_rgb(p, q, h - 1.0 / 3.0),
    );
}

// Mirrors `AdjustmentLut::apply` (`core/src/filters.rs`): the same byte-indexed tone table, the
// same HSL stage, applied to `c` between the sRGB decode `textureSample` already did and the
// premultiply `fs_tile` is about to do. Indexed by `layer_index` rather than a `LayerData`
// value: a row is 1072 bytes, almost all of it the `tone` table, and the overwhelmingly common
// case is `LUT_MODE_IDENTITY` — reading only `layer_data[layer_index].lut_mode` up front (4
// bytes) instead of the whole row means that case never touches `tone` at all. `tone` is
// indexed by the *encoded* byte value — see the comment above `linear_to_srgb` for why this is
// not simply `c.rgb`.
fn apply_adjustments(c: vec4<f32>, layer_index: u32) -> vec4<f32> {
    let lut_mode = layer_data[layer_index].lut_mode;
    if lut_mode == LUT_MODE_IDENTITY {
        return c;
    }
    let encoded = vec3<f32>(
        linear_to_srgb(c.r),
        linear_to_srgb(c.g),
        linear_to_srgb(c.b),
    );
    let byte = vec3<u32>(clamp(round(encoded * 255.0), vec3<f32>(0.0), vec3<f32>(255.0)));
    var v = vec3<f32>(
        layer_data[layer_index].tone[byte.r],
        layer_data[layer_index].tone[byte.g],
        layer_data[layer_index].tone[byte.b],
    );
    if lut_mode == LUT_MODE_TONE_HSL {
        v = hsl_stage(v, layer_data[layer_index].saturation, layer_data[layer_index].vibrance);
    }
    return vec4<f32>(srgb_to_linear(v.r), srgb_to_linear(v.g), srgb_to_linear(v.b), c.a);
}

@fragment
fn fs_tile(input: TileVsOut) -> @location(0) vec4<f32> {
    // Plain `textureSample` (not `...Level`) picks its mip from the screen-space derivatives of
    // `input.uv` automatically — coarser levels as the board zooms out, blended between levels
    // by `tile_sampler`'s mipmap_filter. That is what keeps a zoomed-out pan from shimmering:
    // without mips this would minify raw 256x256 texels with nothing pre-filtered underneath.
    //
    // Past `limits::CRISP_PIXEL_ZOOM` the same sample is a magnification instead, where a
    // bilinear tap smears one texel into a gradient the width of the whole magnified pixel.
    // The crisp sampler differs only in mag_filter, so minification behaves identically either
    // way; `tu.crisp` is uniform, so both arms are uniform control flow.
    var c: vec4<f32>;
    if tu.crisp > 0.5 {
        c = textureSample(tile_tex, tile_sampler_crisp, input.uv, i32(input.slot));
    } else {
        c = textureSample(tile_tex, tile_sampler, input.uv, i32(input.slot));
    }
    c = apply_adjustments(c, input.layer_index);
    c.a *= layer_data[input.layer_index].opacity;
    return vec4<f32>(c.rgb * c.a, c.a);
}

struct SolidVsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) slot: u32,
    @location(1) @interpolate(flat) layer_index: u32,
}

// Paper collapses to one full-document quad when its tiles all share one `Arc`, so there is no
// instance buffer to carry a layer index. The draw passes it as a one-instance range instead —
// `draw(0..6, i..i+1)` — which is why this reads `instance_index` rather than a vertex input.
@vertex
fn vs_doc_quad(@builtin(vertex_index) idx: u32, @builtin(instance_index) layer_index: u32) -> SolidVsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    let uv = corners[idx];
    let doc = uv * tu.doc_size;
    let screen = doc * tu.zoom + tu.pan;
    let device = screen * tu.dpr;
    let ndc = vec2<f32>(
        (device.x / max(tu.viewport.x, 1.0)) * 2.0 - 1.0,
        1.0 - (device.y / max(tu.viewport.y, 1.0)) * 2.0,
    );
    var out: SolidVsOut;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.slot = layer_data[layer_index].atlas_slot;
    out.layer_index = layer_index;
    return out;
}

@fragment
fn fs_solid_tile(input: SolidVsOut) -> @location(0) vec4<f32> {
    var c = textureSample(tile_tex, tile_sampler, vec2<f32>(0.5, 0.5), i32(input.slot));
    c = apply_adjustments(c, input.layer_index);
    c.a *= layer_data[input.layer_index].opacity;
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

fn is_on(flag: f32) -> bool {
    return flag > 0.5;
}

// Mirrors `Tool::takes_fill` in Rust: the tools that enclose an area, and so can carry a
// fill and an outline at once. A line or an arrow has no interior to fill.
fn tool_takes_fill(tool: u32) -> bool {
    return tool == TOOL_RECT || tool == TOOL_ELLIPSE || tool == TOOL_TRIANGLE || tool == TOOL_PENTAGON;
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

// The segment distance (`pa`/`ba`/`h`/`seg`) only feeds TOOL_LINE and TOOL_ARROW, so each
// computes it locally instead of it running unconditionally before the switch — the SDF
// polygon cases (RECT/ELLIPSE/TRIANGLE/PENTAGON) used to pay for a dot product, a clamp and
// a sqrt every pixel for a value they never touch.
fn shape_region(tool: u32, p0: vec2<f32>, p1: vec2<f32>, half_width: f32, p: vec2<f32>) -> f32 {
    switch tool {
        case TOOL_LINE: {
            let pa = p - p0;
            let ba = p1 - p0;
            let baba = dot(ba, ba);
            let h = select(0.0, clamp(dot(pa, ba) / baba, 0.0, 1.0), baba > 0.0);
            return length(pa - ba * h);
        }
        case TOOL_ARROW: {
            let pa = p - p0;
            let ba = p1 - p0;
            let baba = dot(ba, ba);
            let h = select(0.0, clamp(dot(pa, ba) / baba, 0.0, 1.0), baba > 0.0);
            let span = length(ba);
            var d = length(pa - ba * h);
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
            return d;
        }
        case TOOL_RECT: {
            let center = (p0 + p1) * 0.5;
            let half = abs(p1 - p0) * 0.5;
            let d = p - center;
            let q = abs(d) - half;
            return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0);
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
            return ed;
        }
        case TOOL_TRIANGLE: {
            let x0 = min(p0.x, p1.x);
            let y0 = min(p0.y, p1.y);
            let x1 = max(p0.x, p1.x);
            let y1 = max(p0.y, p1.y);
            let v0 = vec2<f32>((x0 + x1) * 0.5, y0);
            let v1 = vec2<f32>(x1, y1);
            let v2 = vec2<f32>(x0, y1);
            return sd_polygon3(p, v0, v1, v2);
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
            return sd_polygon5(p, verts);
        }
        case TOOL_PEN, default: {
            return 1e9;
        }
    }
}

// Straight-alpha `over`. A shape with both a fill and a stroke is two colors in one
// fragment and the alpha blend state can only take one, so the two are composited here — in
// the same order, and to the same result, as the CPU commit's `blend_over` pair.
fn ink_over(bottom: vec4<f32>, top: vec4<f32>) -> vec4<f32> {
    let a = top.a + bottom.a * (1.0 - top.a);
    if a <= 0.0 {
        return vec4<f32>(0.0);
    }
    let rgb = (top.rgb * top.a + bottom.rgb * bottom.a * (1.0 - top.a)) / a;
    return vec4<f32>(rgb, a);
}

fn ink_sample(d: f32, color: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(color.rgb, color.a * clamp(0.5 - d, 0.0, 1.0));
}

// One shape's ink at one pixel, fill under stroke. The CPU twin is `Shape::fill_distance` /
// `stroke_distance` fed through `ink_sample` in `vector.rs` — same region SDF, same
// half-pixel band, same order — so the board and a flattened export agree.
fn shape_ink(
    tool: u32,
    p0: vec2<f32>,
    p1: vec2<f32>,
    half_width: f32,
    fill: f32,
    stroke: f32,
    fill_color: vec4<f32>,
    stroke_color: vec4<f32>,
    p: vec2<f32>,
) -> vec4<f32> {
    let region = shape_region(tool, p0, p1, half_width, p);
    if region > 1e8 {
        return vec4<f32>(0.0);
    }
    if !tool_takes_fill(tool) {
        return ink_sample(region - half_width, stroke_color);
    }
    var out = vec4<f32>(0.0);
    if is_on(fill) {
        out = ink_sample(region, fill_color);
    }
    if is_on(stroke) {
        out = ink_over(out, ink_sample(abs(region) - half_width, stroke_color));
    }
    return out;
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
    shape_stroke: f32,
    stroke_ink: vec4<f32>,
    shape_stroke_color: vec4<f32>,
}

@group(0) @binding(0) var<uniform> pu: PreviewUniforms;

struct StrokeIn {
    @location(0) segment: vec4<f32>,
    @location(1) color: vec4<f32>,
    @location(2) brush: vec4<f32>,
}

struct StrokeOut {
    @builtin(position) position: vec4<f32>,
    @location(0) doc: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) p0: vec2<f32>,
    @location(3) p1: vec2<f32>,
    @location(4) brush: vec4<f32>,
}

fn grain_cell(x: i32, y: i32) -> f32 {
    var h = (bitcast<u32>(x) * 0x27d4eb2du) ^ (bitcast<u32>(y) * 0x165667b1u);
    h = h ^ (h >> 15u);
    h = h * 0x2c1b3c6du;
    h = h ^ (h >> 13u);
    return f32(h & 0xffffu) / 65535.0;
}

fn paper_grain(p: vec2<f32>, scale: f32) -> f32 {
    let g = p / max(scale, 0.25);
    let cell = floor(g);
    let f = g - cell;
    let ease = f * f * (3.0 - 2.0 * f);
    let ix = i32(cell.x);
    let iy = i32(cell.y);
    let top_left = grain_cell(ix, iy);
    let top_right = grain_cell(ix + 1, iy);
    let bottom_left = grain_cell(ix, iy + 1);
    let bottom_right = grain_cell(ix + 1, iy + 1);
    let top = top_left + (top_right - top_left) * ease.x;
    let bottom = bottom_left + (bottom_right - bottom_left) * ease.x;
    return top + (bottom - top) * ease.y;
}

fn stroke_coverage(brush: vec4<f32>, distance: f32, p: vec2<f32>) -> f32 {
    let radius = brush.x;
    let hardness = brush.y;
    let grain = brush.z;
    let feather = max(radius * (1.0 - hardness), 1.0);
    let ramp = clamp((radius + 0.5 - distance) / feather, 0.0, 1.0);
    var shaped = ramp;
    if hardness < 1.0 {
        shaped = ramp * ramp * (3.0 - 2.0 * ramp);
    }
    if grain <= 0.0 || shaped <= 0.0 {
        return shaped;
    }
    return shaped * (1.0 - grain * (1.0 - paper_grain(p, brush.w)));
}

fn stroke_distance(input: StrokeOut) -> f32 {
    let pa = input.doc - input.p0;
    let ba = input.p1 - input.p0;
    let baba = dot(ba, ba);
    let h = select(0.0, clamp(dot(pa, ba) / baba, 0.0, 1.0), baba > 0.0);
    return length(pa - ba * h);
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
    let pad = vec2<f32>(input.brush.x + 1.0);
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
    out.brush = input.brush;
    return out;
}

@fragment
fn fs_stroke(input: StrokeOut) -> @location(0) vec4<f32> {
    let cov = stroke_coverage(input.brush, stroke_distance(input), input.doc);
    if cov <= 0.0 {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(input.color.rgb, input.color.a * cov);
}

@fragment
fn fs_stroke_coverage(input: StrokeOut) -> @location(0) vec4<f32> {
    return vec4<f32>(stroke_coverage(input.brush, stroke_distance(input), input.doc));
}

@group(1) @binding(0) var stroke_cov_tex: texture_2d<f32>;

@fragment
fn fs_stroke_composite(in: VsOut) -> @location(0) vec4<f32> {
    let cov = textureLoad(stroke_cov_tex, vec2<i32>(in.position.xy), 0).r;
    if cov <= 0.0 {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(pu.stroke_ink.rgb, pu.stroke_ink.a * cov);
}

struct GuideIn {
    @location(0) segment: vec4<f32>,
    @location(1) color: vec4<f32>,
}

struct GuideOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) p0: vec2<f32>,
    @location(2) p1: vec2<f32>,
}

// A guide is chrome, not content: it stays one screen pixel wide at every zoom, the way the
// ruler ticks it is pulled from do. That is why it gets its own pair of entry points instead of
// riding the stroke pass — `vs_stroke` measures its brush in document units, so a rule thin
// enough at 1:1 would smear into a wide gradient once the board is zoomed in.
const GUIDE_HALF_WIDTH_PX: f32 = 0.5;
const GUIDE_QUAD_PAD_PX: f32 = 2.0;

@vertex
fn vs_guide(input: GuideIn, @builtin(vertex_index) idx: u32) -> GuideOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    let a = input.segment.xy * pu.zoom + pu.pan;
    let b = input.segment.zw * pu.zoom + pu.pan;
    let pad = vec2<f32>(GUIDE_HALF_WIDTH_PX + GUIDE_QUAD_PAD_PX);
    let lo = min(a, b) - pad;
    let hi = max(a, b) + pad;
    let screen = mix(lo, hi, corners[idx]);
    let device = screen * pu.dpr;
    let ndc = vec2<f32>(
        (device.x / max(pu.viewport.x, 1.0)) * 2.0 - 1.0,
        1.0 - (device.y / max(pu.viewport.y, 1.0)) * 2.0,
    );
    var out: GuideOut;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.color = input.color;
    out.p0 = a;
    out.p1 = b;
    return out;
}

@fragment
fn fs_guide(input: GuideOut) -> @location(0) vec4<f32> {
    let screen = input.position.xy / max(pu.dpr, 1.0);
    let d = sd_segment_pts(screen, input.p0, input.p1);
    let cov = clamp(GUIDE_HALF_WIDTH_PX + 0.5 - d, 0.0, 1.0);
    if cov <= 0.0 {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(input.color.rgb, input.color.a * cov);
}

struct OverlayOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) p0: vec2<f32>,
    @location(2) p1: vec2<f32>,
    @location(3) @interpolate(flat) half_width_px: f32,
}

// Board furniture — transform grips, the frames they sit on, the layer hover outline — measured
// in screen pixels instead of document units, for the same reason `vs_guide` exists: chrome is
// the same size at every zoom or it is not chrome. It takes the stroke pass's own instance
// shape so nothing has to build a second kind of overlay, and reads `brush.x` as a screen-pixel
// half-width rather than a document-unit brush radius. The SDF is `sd_segment_pts`, the one
// `fs_guide` already evaluates — a new entry point over existing geometry, not new geometry.
const OVERLAY_QUAD_PAD_PX: f32 = 2.0;

@vertex
fn vs_overlay(input: StrokeIn, @builtin(vertex_index) idx: u32) -> OverlayOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    let a = input.segment.xy * pu.zoom + pu.pan;
    let b = input.segment.zw * pu.zoom + pu.pan;
    let pad = vec2<f32>(input.brush.x + OVERLAY_QUAD_PAD_PX);
    let lo = min(a, b) - pad;
    let hi = max(a, b) + pad;
    let screen = mix(lo, hi, corners[idx]);
    let device = screen * pu.dpr;
    let ndc = vec2<f32>(
        (device.x / max(pu.viewport.x, 1.0)) * 2.0 - 1.0,
        1.0 - (device.y / max(pu.viewport.y, 1.0)) * 2.0,
    );
    var out: OverlayOut;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.color = input.color;
    out.p0 = a;
    out.p1 = b;
    out.half_width_px = input.brush.x;
    return out;
}

@fragment
fn fs_overlay(input: OverlayOut) -> @location(0) vec4<f32> {
    let screen = input.position.xy / max(pu.dpr, 1.0);
    let d = sd_segment_pts(screen, input.p0, input.p1);
    let cov = clamp(input.half_width_px + 0.5 - d, 0.0, 1.0);
    if cov <= 0.0 {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(input.color.rgb, input.color.a * cov);
}

const ARROW_HEAD_RATIO: f32 = 6.0;
const ARROW_HEAD_MIN: f32 = 10.0;
const ARROW_HEAD_MAX: f32 = 80.0;

struct VectorShapeIn {
    @location(0) p0: vec2<f32>,
    @location(1) p1: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) stroke_color: vec4<f32>,
    @location(4) half_width: f32,
    @location(5) tool: f32,
    @location(6) fill: f32,
    @location(7) stroke: f32,
}

struct VectorShapeOut {
    @builtin(position) position: vec4<f32>,
    @location(0) doc: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) p0: vec2<f32>,
    @location(3) p1: vec2<f32>,
    @location(4) half_width: f32,
    @location(5) tool: f32,
    @location(6) fill: f32,
    @location(7) stroke_color: vec4<f32>,
    @location(8) stroke: f32,
}

@vertex
fn vs_vector_shape(input: VectorShapeIn, @builtin(vertex_index) idx: u32) -> VectorShapeOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    var head = 0.0;
    if u32(input.tool + 0.5) == TOOL_ARROW {
        head = clamp(input.half_width * ARROW_HEAD_RATIO, ARROW_HEAD_MIN, ARROW_HEAD_MAX);
    }
    let pad = vec2<f32>(input.half_width + head + 1.0);
    let lo = min(input.p0, input.p1) - pad;
    let hi = max(input.p0, input.p1) + pad;
    let doc = mix(lo, hi, corners[idx]);
    let screen = doc * pu.zoom + pu.pan;
    let device = screen * pu.dpr;
    let ndc = vec2<f32>(
        (device.x / max(pu.viewport.x, 1.0)) * 2.0 - 1.0,
        1.0 - (device.y / max(pu.viewport.y, 1.0)) * 2.0,
    );
    var out: VectorShapeOut;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.doc = doc;
    out.color = input.color;
    out.p0 = input.p0;
    out.p1 = input.p1;
    out.half_width = input.half_width;
    out.tool = input.tool;
    out.fill = input.fill;
    out.stroke_color = input.stroke_color;
    out.stroke = input.stroke;
    return out;
}

@fragment
fn fs_vector_shape(input: VectorShapeOut) -> @location(0) vec4<f32> {
    return shape_ink(
        u32(input.tool + 0.5),
        input.p0,
        input.p1,
        input.half_width,
        input.fill,
        input.stroke,
        input.color,
        input.stroke_color,
        input.doc,
    );
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
    return shape_ink(
        u32(pu.tool + 0.5),
        pu.p0,
        pu.p1,
        pu.half_width,
        pu.fill,
        pu.shape_stroke,
        pu.color,
        pu.shape_stroke_color,
        xy,
    );
}

struct OverviewCamera {
    pan: vec2<f32>,
    zoom: f32,
    dpr: f32,
    viewport: vec2<f32>,
    doc_size: vec2<f32>,
    _pad: vec2<f32>,
}

@group(0) @binding(0) var<uniform> ou: OverviewCamera;
@group(0) @binding(1) var overview_tex: texture_2d<f32>;
@group(0) @binding(2) var overview_sampler: sampler;

struct OverviewVsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_overview(@builtin(vertex_index) idx: u32) -> OverviewVsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    let uv = corners[idx];
    let doc = uv * ou.doc_size;
    let screen = doc * ou.zoom + ou.pan;
    let device = screen * ou.dpr;
    let ndc = vec2<f32>(
        (device.x / max(ou.viewport.x, 1.0)) * 2.0 - 1.0,
        1.0 - (device.y / max(ou.viewport.y, 1.0)) * 2.0,
    );
    var out: OverviewVsOut;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_overview(in: OverviewVsOut) -> @location(0) vec4<f32> {
    let c = textureSample(overview_tex, overview_sampler, in.uv);
    return vec4<f32>(c.rgb * c.a, c.a);
}

@group(0) @binding(0) var pan_cache_tex: texture_2d<f32>;
@group(0) @binding(1) var pan_cache_sampler: sampler;

struct BlitVsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// Content textures are sized 1:1 with the swapchain every frame (`PanCache::resize`), so a
// bare NDC → UV remap lines the sample up with the destination pixel exactly — no camera
// uniform needed for this pass.
@vertex
fn vs_blit(@builtin(vertex_index) idx: u32) -> BlitVsOut {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var out: BlitVsOut;
    let p = pos[idx];
    out.position = vec4<f32>(p, 0.0, 1.0);
    out.uv = vec2<f32>(p.x * 0.5 + 0.5, 1.0 - (p.y * 0.5 + 0.5));
    return out;
}

@fragment
fn fs_blit(in: BlitVsOut) -> @location(0) vec4<f32> {
    return textureSample(pan_cache_tex, pan_cache_sampler, in.uv);
}

@fragment
fn fs_clear_transparent() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, 0.0);
}
