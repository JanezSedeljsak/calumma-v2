use super::tests::{fixture, pixel, Fixture};
use super::*;
use crate::renderer::pipeline::{premultiplied_target, TILE_INSTANCE_ATTRS};
use crate::test_gpu::{gpu, read_texture_layer, Gpu};
use calumma_core::filters::{AdjustmentLut, Adjustments};
use calumma_core::tile::{TILE_BYTES, TILE_SIZE};

/// Renders one tile of `combos.len()` distinct texels (row-major, one combo per texel)
/// through `fs_tile` with `row` as its only `LayerData` entry, and hands back the target's
/// pixels. The target is a *separate* sRGB texture, not `Fixture::TARGET_FORMAT`: `fs_tile`
/// hands back linear light for correct blending (see the comment above `linear_to_srgb` in
/// board.wgsl), and only an sRGB target's automatic re-encode on write turns that back into
/// the same sRGB-encoded byte `AdjustmentLut::apply` computes on the CPU.
fn render_byte_cube(gpu: &Gpu, f: &Fixture, slot: u32, row: LayerData) -> Vec<u8> {
    f.write_rows(gpu, &[row]);

    let srgb_format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("byte-cube-target"),
        size: wgpu::Extent3d {
            width: TILE_SIZE,
            height: TILE_SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: srgb_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let layout = gpu
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("byte-cube-pl"),
            bind_group_layouts: &[Some(&f.bgl)],
            ..Default::default()
        });
    let pipe = gpu
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("byte-cube-test"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &gpu.shader,
                entry_point: Some("vs_tile"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<TileInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: TILE_INSTANCE_ATTRS,
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &gpu.shader,
                entry_point: Some("fs_tile"),
                compilation_options: Default::default(),
                targets: &[Some(premultiplied_target(srgb_format))],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

    let instance = TileInstance {
        origin: [0.0, 0.0],
        slot,
        layer_index: 0,
    };
    let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("byte-cube-instance"),
        size: std::mem::size_of::<TileInstance>() as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    gpu.queue
        .write_buffer(&buf, 0, bytemuck::bytes_of(&instance));

    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("byte-cube-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipe);
        pass.set_bind_group(0, f.atlas.bind_group(), &[]);
        pass.set_vertex_buffer(0, buf.slice(..));
        pass.draw(0..6, 0..1);
    }
    gpu.queue.submit(Some(encoder.finish()));
    read_texture_layer(&gpu.device, &gpu.queue, &target, 0, TILE_SIZE)
}

/// `fs_tile`'s `apply_adjustments` and `core::filters::AdjustmentLut::apply` are two
/// independent implementations of the same math — one WGSL, one Rust — kept in step by
/// hand. This is the test that actually enforces it, over a stratified sample of the byte
/// cube covering both `LUT_MODE_TONE` (tone only) and `LUT_MODE_TONE_HSL` (tone + hue/sat).
/// A 1-of-255 tolerance absorbs the sRGB round trip: `apply_adjustments` undoes the atlas
/// texture's automatic sRGB decode in software (`linear_to_srgb`/`srgb_to_linear`) so the
/// lookup lands on the same byte the CPU path would use, and redoes it in software before
/// the GPU's own hardware re-encodes on write to the sRGB target — two curves computed two
/// different ways, not required to be bit-identical.
#[test]
fn fs_tile_adjustments_agree_with_the_cpu_lut_over_a_byte_cube() {
    let Some(gpu) = gpu() else { return };
    let mut f = fixture(gpu);

    const STEPS: [u8; 9] = [0, 32, 64, 96, 128, 160, 192, 224, 255];
    let mut combos: Vec<[u8; 3]> = Vec::new();
    for &r in &STEPS {
        for &g in &STEPS {
            for &b in &STEPS {
                combos.push([r, g, b]);
            }
        }
    }
    assert!(combos.len() <= (TILE_SIZE * TILE_SIZE) as usize);

    let mut base = vec![0u8; TILE_BYTES];
    for (i, rgb) in combos.iter().enumerate() {
        let px = i * 4;
        base[px] = rgb[0];
        base[px + 1] = rgb[1];
        base[px + 2] = rgb[2];
        base[px + 3] = 255;
    }
    let shared = SharedBindings {
        layout: &f.bgl,
        camera: &f.camera,
        layers: &f.layers,
        samplers: &f.samplers,
    };
    let slot = f
        .atlas
        .allocate(&gpu.device, &gpu.queue, &shared)
        .expect("slot");
    f.atlas.write(&gpu.queue, slot, &base, &[]);

    for adjustments in [
        // Tone only: saturation, vibrance and hue neutral, so `write_layer_data` would pick
        // `LUT_MODE_TONE` and the shader never enters `hsl_stage`.
        Adjustments {
            brightness: 0.15,
            contrast: 0.2,
            vibrance: 0.0,
            saturation: 0.0,
            levels_gamma: 1.4,
            hue: 0.0,
        },
        // Tone + HSL: exercises `rgb_to_hsl` / `hue_to_rgb` / `hsl_to_rgb` too.
        Adjustments {
            brightness: 0.15,
            contrast: 0.2,
            vibrance: 0.3,
            saturation: -0.25,
            levels_gamma: 1.4,
            hue: 0.0,
        },
        // Tone + HSL again, this time driven by hue alone, to catch the shader's hue
        // rotation disagreeing with `hsl_stage` in `core/src/filters.rs` specifically.
        Adjustments {
            brightness: 0.0,
            contrast: 0.0,
            vibrance: 0.0,
            saturation: 0.0,
            levels_gamma: 1.0,
            hue: 120.0,
        },
    ] {
        let lut = AdjustmentLut::new(&adjustments);
        let row = if lut.is_tone_only() {
            LayerData {
                tone: *lut.tone_table(),
                lut_mode: LUT_MODE_TONE,
                ..LayerData::default()
            }
        } else {
            LayerData {
                tone: *lut.tone_table(),
                lut_mode: LUT_MODE_TONE_HSL,
                saturation: adjustments.saturation,
                vibrance: adjustments.vibrance,
                hue: adjustments.hue,
                ..LayerData::default()
            }
        };

        let image = render_byte_cube(gpu, &f, slot, row);

        let mut max_diff = 0i32;
        for (i, rgb) in combos.iter().enumerate() {
            let expected = lut.apply(*rgb);
            let got = pixel(&image, (i as u32) % TILE_SIZE, (i as u32) / TILE_SIZE);
            assert_eq!(got[3], 255, "alpha is untouched by adjustments");
            for c in 0..3 {
                max_diff = max_diff.max((got[c] as i32 - expected[c] as i32).abs());
            }
        }
        assert!(
            max_diff <= 1,
            "GPU and CPU adjustments disagree by more than 1 of 255 somewhere (max {max_diff}, lut_mode {})",
            row.lut_mode
        );
    }
}
