use super::*;
use crate::compose::{brush_params, StrokeInstance};
use crate::renderer::{PreviewUniforms, STROKE_ATTRS};
use bytemuck::Zeroable;
use calumma_core::brush::{segment_distance, stroke_coverage, Brush, BrushProfile};

const SIDE: u32 = 96;

struct Harness {
    device: wgpu::Device,
    queue: wgpu::Queue,
    coverage: StrokeCoverage,
    preview_bg: wgpu::BindGroup,
}

fn harness() -> Option<Harness> {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .ok()?;
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()?;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("board"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/board.wgsl").into()),
    });
    let preview_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("preview-bgl"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let mut uniforms = PreviewUniforms::zeroed();
    uniforms.zoom = 1.0;
    uniforms.dpr = 1.0;
    uniforms.viewport = [SIDE as f32, SIDE as f32];
    let preview_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("preview-uniform"),
        size: std::mem::size_of::<PreviewUniforms>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&preview_buf, 0, bytemuck::bytes_of(&uniforms));
    let preview_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("preview-bg"),
        layout: &preview_bgl,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: preview_buf.as_entire_binding(),
        }],
    });
    let mut coverage = StrokeCoverage::new(
        &device,
        &shader,
        &preview_bgl,
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<StrokeInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: STROKE_ATTRS,
        },
        wgpu::TextureFormat::Bgra8UnormSrgb,
    );
    coverage.ensure(&device, SIDE, SIDE);
    Some(Harness {
        device,
        queue,
        coverage,
        preview_bg,
    })
}

fn render(h: &Harness, instances: &[StrokeInstance]) -> Vec<u8> {
    render_in_batches(h, &[instances])
}

/// The same rasterize, split across as many `accumulate` calls as there are batches — one
/// per simulated frame, each in its own encoder and submission, with only the first
/// restarting. This is the shape the renderer drives the target in during a live stroke.
fn render_in_batches(h: &Harness, batches: &[&[StrokeInstance]]) -> Vec<u8> {
    let instances: Vec<StrokeInstance> = batches.concat();
    let buf = h.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("instances"),
        size: std::mem::size_of_val(instances.as_slice()).max(1) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    h.queue
        .write_buffer(&buf, 0, bytemuck::cast_slice(&instances));

    let mut first = 0u32;
    for (i, batch) in batches.iter().enumerate() {
        let end = first + batch.len() as u32;
        let mut encoder = h
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        h.coverage
            .accumulate(&mut encoder, &h.preview_bg, &buf, first..end, None, i == 0);
        h.queue.submit(Some(encoder.finish()));
        first = end;
    }

    let row = align_row(SIDE);
    let readback = h.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (row * SIDE) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = h
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let target = h.coverage.target.as_ref().expect("target");
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &target.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(SIDE),
            },
        },
        wgpu::Extent3d {
            width: SIDE,
            height: SIDE,
            depth_or_array_layers: 1,
        },
    );
    h.queue.submit(Some(encoder.finish()));

    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    h.device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .expect("poll");
    let mapped = slice.get_mapped_range().expect("mapped");
    let mut out = vec![0u8; (SIDE * SIDE) as usize];
    for y in 0..SIDE as usize {
        let src = y * row as usize;
        let dst = y * SIDE as usize;
        out[dst..dst + SIDE as usize].copy_from_slice(&mapped[src..src + SIDE as usize]);
    }
    drop(mapped);
    readback.unmap();
    out
}

fn align_row(width: u32) -> u32 {
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    width.div_ceil(align) * align
}

fn at(pixels: &[u8], x: u32, y: u32) -> u8 {
    pixels[(y * SIDE + x) as usize]
}

fn segment(a: (f32, f32), b: (f32, f32), radius: f32, profile: &BrushProfile) -> StrokeInstance {
    StrokeInstance {
        segment: [a.0, a.1, b.0, b.1],
        color: [1.0, 1.0, 1.0, 1.0],
        brush: brush_params(radius, profile),
    }
}

/// The whole reason this target exists. Two soft capsules whose tails overlap must union,
/// not sum: summing is exactly the compounding that turned a low-opacity stroke into a
/// beaded rope, and it is what a plain alpha blend would do.
#[test]
fn overlapping_segments_union_rather_than_sum() {
    let Some(h) = harness() else {
        return;
    };
    let soft = BrushProfile {
        hardness: 0.0,
        flow: 1.0,
        grain: 0.0,
        grain_scale: 1.0,
    };
    let radius = 10.0;
    let above = segment((20.0, 27.5), (76.0, 27.5), radius, &soft);
    let below = segment((20.0, 43.5), (76.0, 43.5), radius, &soft);

    let alone = render(&h, &[above]);
    let both = render(&h, &[above, below]);

    let one = at(&alone, 48, 35);
    let two = at(&both, 48, 35);
    assert!(
        (16..96).contains(&one),
        "the sample sits equidistant in both soft tails, faint enough that a sum would              still have headroom to show: got {one}"
    );
    assert_eq!(
        one, two,
        "a second segment reaching the same pixel just as faintly must not darken it —              {one} + {one} is what an alpha blend would have given"
    );
}

/// The brush profile travels to the shader in the instance's `brush` vector. If that
/// widened vertex attribute were wired up wrong, every brush would silently draw as a hard
/// pen — which looks fine until you try to use one.
#[test]
fn hardness_reaches_the_shader() {
    let Some(h) = harness() else {
        return;
    };
    let hard = render(
        &h,
        &[segment(
            (20.0, 48.0),
            (76.0, 48.0),
            12.0,
            &BrushProfile::HARD,
        )],
    );
    let soft = render(
        &h,
        &[segment(
            (20.0, 48.0),
            (76.0, 48.0),
            12.0,
            &BrushProfile {
                hardness: 0.0,
                flow: 1.0,
                grain: 0.0,
                grain_scale: 1.0,
            },
        )],
    );
    assert_eq!(at(&hard, 48, 54), 255, "a hard edge is full up to its rim");
    assert!(
        at(&soft, 48, 54) < 200,
        "a soft one has fallen off well before it, got {}",
        at(&soft, 48, 54)
    );
    assert_eq!(at(&hard, 48, 48), 255);
    assert_eq!(at(&soft, 48, 48), 255, "both are solid at the centre line");
}

/// Crayon grain is document-space value noise, so a grainy stroke's coverage varies along
/// its own axis where a smooth one is constant.
#[test]
fn grain_reaches_the_shader() {
    let Some(h) = harness() else {
        return;
    };
    let crayon = Brush::Crayon.profile();
    let grainy = render(&h, &[segment((20.0, 48.0), (76.0, 48.0), 12.0, &crayon)]);
    let smooth = render(
        &h,
        &[segment(
            (20.0, 48.0),
            (76.0, 48.0),
            12.0,
            &BrushProfile::HARD,
        )],
    );

    let axis: Vec<u8> = (30..70).map(|x| at(&grainy, x, 48)).collect();
    let flat: Vec<u8> = (30..70).map(|x| at(&smooth, x, 48)).collect();
    assert!(
        flat.iter().all(|&v| v == 255),
        "the smooth brush is constant along its axis"
    );
    let low = *axis.iter().min().unwrap();
    let high = *axis.iter().max().unwrap();
    assert!(
        high - low > 20,
        "the crayon's tooth should show along the axis, got {low}..{high}"
    );
}

/// The append contract this whole target rests on: unioning segment N onto the union of
/// 0..N has to land on exactly the same pixels as unioning 0..N+1 from an empty target.
/// `Max` is idempotent and order-independent, so this is not an approximation — a single
/// byte of drift here would mean a live stroke looked different depending on how fast the
/// pointer happened to move.
#[test]
fn appending_a_segment_at_a_time_matches_one_full_rasterize() {
    let Some(h) = harness() else {
        return;
    };
    let profile = Brush::Marker.profile();
    let points = [
        (18.0, 30.0),
        (32.0, 46.0),
        (48.0, 34.0),
        (62.0, 52.0),
        (78.0, 40.0),
    ];
    let capsules: Vec<StrokeInstance> = points
        .windows(2)
        .map(|p| segment(p[0], p[1], 9.0, &profile))
        .collect();

    let at_once = render(&h, &capsules);
    let batches: Vec<&[StrokeInstance]> = capsules.chunks(1).collect();
    let appended = render_in_batches(&h, &batches);

    assert_eq!(
        at_once, appended,
        "a stroke accumulated one segment per frame must be byte-identical to the same           stroke rasterized in one pass"
    );
}

/// Restarting is the *only* way coverage comes back out of the target — `Max` cannot
/// subtract — which is why `Document::stroke_generation` bumps when a Shift-held straight
/// segment rewinds the point list. This is the rewind the renderer has to notice: appending
/// over it leaves the abandoned capsule standing.
#[test]
fn a_rewound_tail_needs_a_restart_to_disappear() {
    let Some(h) = harness() else {
        return;
    };
    let profile = BrushProfile::HARD;
    let anchor = (20.0, 48.0);
    let abandoned = segment(anchor, (76.0, 20.0), 6.0, &profile);
    let kept = segment(anchor, (76.0, 76.0), 6.0, &profile);

    let appended = render_in_batches(&h, &[&[abandoned], &[kept]]);
    let restarted = render(&h, &[kept]);

    assert!(
        at(&appended, 70, 24) > 0,
        "appending leaves the abandoned segment in the target — the failure mode the             generation bump exists to prevent"
    );
    assert_eq!(
        at(&restarted, 70, 24),
        0,
        "a restart wipes it, and the kept segment does not reach there"
    );
    assert!(
        at(&restarted, 70, 72) > 0,
        "the kept segment is still drawn"
    );
}

/// The strongest check available: the shader and `brush.rs` must compute the *same*
/// coverage, or the stroke would change the instant the pointer came up. Every brush is
/// sampled across its whole falloff, not just at the centre.
#[test]
fn the_shader_matches_the_engines_own_coverage() {
    let Some(h) = harness() else {
        return;
    };
    let radius = 14.0;
    let a = (24.0, 48.0);
    let b = (72.0, 48.0);
    for brush in [Brush::Pen, Brush::Marker, Brush::Crayon, Brush::Airbrush] {
        let profile = brush.profile();
        let pixels = render(&h, &[segment(a, b, radius, &profile)]);
        for y in 34..=62 {
            for x in [30u32, 48, 66] {
                let p = (x as f32 + 0.5, y as f32 + 0.5);
                let expected =
                    stroke_coverage(&profile, segment_distance(p, a, b), radius, p.0, p.1);
                let expected = (expected * 255.0).round().clamp(0.0, 255.0) as i32;
                let actual = at(&pixels, x, y) as i32;
                assert!(
                    (expected - actual).abs() <= 2,
                    "{brush:?} disagrees at ({x},{y}): engine {expected}, shader {actual}"
                );
            }
        }
    }
}
