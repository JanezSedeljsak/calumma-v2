//! The desk lattice, baked once instead of evaluated per pixel.
//!
//! `fs_paper` is a fullscreen triangle drawn on every frame the board renders at all, and its
//! grid used to cost ~30 scalar ops and six branches at every one of those pixels — on a 4K
//! viewport, a quarter of a billion operations a frame for a background nobody is looking at.
//! On an integrated GPU that is the single largest thing the board does when it is not being
//! drawn on.
//!
//! It does not have to be. `desk_pattern` reads *only* `screen`: the desk is deliberately
//! screen-locked (`docs/RENDERING.md` — it does not scroll with the board and does not scale
//! with zoom), and both halves of the pattern — the cell rules and the corner crosses — repeat
//! with period [`DeskMetrics::cell`], anchored at the viewport's own origin. So one period's
//! worth of texels, addressed by device pixel modulo that period, reproduces the whole
//! viewport exactly.
//!
//! The two halves stay in separate channels rather than being pre-blended, because they mix
//! toward the grid color at different strengths and that color and its alpha are theme
//! uniforms: baking the blend would mean re-baking on every theme switch, and the composition
//! of two `mix`es toward the same color is not linear in that alpha anyway. Red carries the
//! rules, green the crosses, and `fs_paper` performs exactly the two mixes it always did.
//!
//! Only `dpr` sizes the texture, and `DeskMetrics` is a compile-time constant, so this is
//! rebuilt on a backing-scale change and never again.

use calumma_core::DeskMetrics;

pub(crate) const DESK_LATTICE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg8Unorm;

/// How far `cell * dpr` may sit from a whole number of device pixels and still tile. A lattice
/// whose period is not an integer count of texels drifts out of phase across the viewport —
/// visibly, since the pattern is a hard-edged grid — so a backing scale that does not land on
/// one falls back to evaluating the pattern in the shader.
///
/// The condition is on the *product*, not on `dpr` alone: a 26pt cell tiles at 1.5x as readily
/// as at 1x or 2x (39 texels). The fallback is a correctness guard for a scale that happens not
/// to divide, not a path any normal display is expected to take.
const LATTICE_PHASE_TOLERANCE: f32 = 1e-3;

/// Side of the baked lattice in device texels, or `None` where the period does not tile.
pub(crate) fn lattice_side(metrics: DeskMetrics, dpr: f32) -> Option<u32> {
    if !dpr.is_finite() || dpr <= 0.0 {
        return None;
    }
    let exact = metrics.cell.max(1.0) * dpr;
    let side = exact.round();
    if side < 1.0 || (exact - side).abs() > LATTICE_PHASE_TOLERANCE {
        return None;
    }
    Some(side as u32)
}

/// One period of the lattice as `Rg8Unorm` texels: red is "on a cell rule", green is "on a
/// corner cross", both hard 0 or 255 so the sampled result is byte-identical to the branch it
/// replaces rather than a smoothed approximation of it.
///
/// Texel `(x, y)` stands for the device pixel at that offset into the period, whose centre is
/// at `(x + 0.5, y + 0.5)` — the same `@builtin(position)` the shader would have seen — divided
/// by `dpr` to reach the logical screen units `desk_pattern` measures in.
pub(crate) fn lattice_texels(metrics: DeskMetrics, dpr: f32) -> Option<(u32, Vec<u8>)> {
    let side = lattice_side(metrics, dpr)?;
    let mut out = vec![0u8; (side as usize) * (side as usize) * 2];
    for y in 0..side {
        for x in 0..side {
            let screen = ((x as f32 + 0.5) / dpr, (y as f32 + 0.5) / dpr);
            let i = ((y * side + x) * 2) as usize;
            out[i] = u8::from(on_rule(metrics, screen)) * u8::MAX;
            out[i + 1] = u8::from(on_cross(metrics, screen)) * u8::MAX;
        }
    }
    Some((side, out))
}

fn on_rule(metrics: DeskMetrics, screen: (f32, f32)) -> bool {
    let cell = metrics.cell.max(1.0);
    let local = (
        screen.0 - (screen.0 / cell).floor() * cell,
        screen.1 - (screen.1 / cell).floor() * cell,
    );
    local.0 < metrics.line_width || local.1 < metrics.line_width
}

fn on_cross(metrics: DeskMetrics, screen: (f32, f32)) -> bool {
    let cell = metrics.cell.max(1.0);
    let local = (
        screen.0 - (screen.0 / cell).round() * cell,
        screen.1 - (screen.1 / cell).round() * cell,
    );
    let half = metrics.cross_line_width * 0.5;
    (local.0.abs() < half && local.1.abs() < metrics.cross_arm)
        || (local.1.abs() < half && local.0.abs() < metrics.cross_arm)
}

/// The baked period, plus the 1×1 stand-in that keeps the paper bind group complete while the
/// shader is on its fallback path — a bind group cannot leave an entry unfilled, and the
/// alternative would be a second pipeline for a case nobody hits.
pub(crate) struct DeskLattice {
    view: wgpu::TextureView,
    side: u32,
    dpr: f32,
}

impl DeskLattice {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue, dpr: f32) -> Self {
        let mut lattice = Self {
            view: placeholder(device),
            side: 0,
            dpr: f32::NAN,
        };
        lattice.rebuild(device, queue, dpr);
        lattice
    }

    pub(crate) fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// What `fs_paper` reads as `lattice_side`: zero means "no usable period, evaluate the
    /// pattern yourself".
    pub(crate) fn shader_side(&self) -> f32 {
        self.side as f32
    }

    /// Rebakes when the backing scale moved, reporting whether the bind group has to be built
    /// again against the new view.
    pub(crate) fn ensure(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, dpr: f32) -> bool {
        if self.dpr == dpr {
            return false;
        }
        self.rebuild(device, queue, dpr);
        true
    }

    fn rebuild(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, dpr: f32) {
        self.dpr = dpr;
        let Some((side, texels)) = lattice_texels(DeskMetrics::DEFAULT, dpr) else {
            self.view = placeholder(device);
            self.side = 0;
            return;
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("desk-lattice"),
            size: wgpu::Extent3d {
                width: side,
                height: side,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DESK_LATTICE_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &texels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(side * 2),
                rows_per_image: Some(side),
            },
            wgpu::Extent3d {
                width: side,
                height: side,
                depth_or_array_layers: 1,
            },
        );
        self.view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.side = side;
    }
}

fn placeholder(device: &wgpu::Device) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("desk-lattice-placeholder"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DESK_LATTICE_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::PaperUniforms;

    /// The backing scales a real display reports. A 26pt cell divides at every one of them,
    /// half-steps included — the condition is on `cell * dpr`, not on `dpr` being whole.
    #[test]
    fn the_period_is_a_whole_number_of_device_pixels_at_the_usual_backing_scales() {
        assert_eq!(lattice_side(DeskMetrics::DEFAULT, 1.0), Some(26));
        assert_eq!(lattice_side(DeskMetrics::DEFAULT, 1.5), Some(39));
        assert_eq!(lattice_side(DeskMetrics::DEFAULT, 2.0), Some(52));
        assert_eq!(lattice_side(DeskMetrics::DEFAULT, 3.0), Some(78));
    }

    /// A period that does not tile drifts out of phase across the viewport, which on a
    /// hard-edged grid is visible. Better to pay the shader for it than to draw it wrong.
    #[test]
    fn a_backing_scale_that_does_not_tile_has_no_lattice() {
        assert_eq!(lattice_side(DeskMetrics::DEFAULT, 1.25), None);
        assert_eq!(lattice_side(DeskMetrics::DEFAULT, 0.0), None);
        assert_eq!(lattice_side(DeskMetrics::DEFAULT, -2.0), None);
        assert_eq!(lattice_side(DeskMetrics::DEFAULT, f32::NAN), None);
    }

    #[test]
    fn the_bake_is_one_period_square_in_two_channels() {
        let (side, texels) = lattice_texels(DeskMetrics::DEFAULT, 2.0).expect("tiles");
        assert_eq!(side, 52);
        assert_eq!(texels.len(), 52 * 52 * 2);
        assert!(
            texels.iter().all(|&v| v == 0 || v == u8::MAX),
            "coverage is a hard test, not a filtered one — the sampled result has to be           byte-identical to the branch it replaces"
        );
    }

    /// The lattice is anchored at the viewport origin, so the first rule of each cell runs
    /// along the top and left edges of the period and the cross sits on the same corner.
    #[test]
    fn the_rules_and_the_cross_land_where_the_pattern_puts_them() {
        let (side, texels) = lattice_texels(DeskMetrics::DEFAULT, 2.0).expect("tiles");
        let at = |x: u32, y: u32| {
            let i = ((y * side + x) * 2) as usize;
            (texels[i], texels[i + 1])
        };

        assert_eq!(at(0, 0).0, u8::MAX, "a rule runs along the cell's own edge");
        assert_eq!(at(0, 0).1, u8::MAX, "and the cross sits on the corner");
        assert_eq!(at(26, 26), (0, 0), "the middle of the cell is bare desk");
        assert_eq!(
            at(1, 30).0,
            u8::MAX,
            "the left rule is two device pixels wide"
        );
        assert_eq!(at(2, 30).0, 0, "and no wider");
        assert_eq!(
            at(0, 30).1,
            0,
            "the cross arm reaches 3.5pt, well short of halfway down the cell"
        );
    }

    const VIEW_W: u32 = 200;
    const VIEW_H: u32 = 140;

    /// `fs_paper` against an offscreen target, with the lattice either baked (`lattice_side`
    /// set) or left for the shader to evaluate.
    fn draw_desk(gpu: &crate::test_gpu::Gpu, dpr: f32, baked: bool) -> Vec<u8> {
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let bgl = gpu
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("paper-bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });
        let lattice = DeskLattice::new(&gpu.device, &gpu.queue, dpr);
        let uniforms = PaperUniforms {
            pan: [0.0, 0.0],
            zoom: 1.0,
            dpr,
            doc_size: [1e9, 1e9],
            viewport: [VIEW_W as f32, VIEW_H as f32],
            dark: 0.0,
            lattice_side: if baked { lattice.shader_side() } else { 0.0 },
            _pad1: 0.0,
            _pad2: 0.0,
            desk_metrics: [
                DeskMetrics::DEFAULT.cell,
                DeskMetrics::DEFAULT.line_width,
                DeskMetrics::DEFAULT.cross_arm,
                DeskMetrics::DEFAULT.cross_line_width,
            ],
            desk: [0.1, 0.12, 0.14, 1.0],
            grid: [0.9, 0.85, 0.8, 0.7],
            paper_border: [0.0, 0.0, 0.0, 0.0],
        };
        let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("paper-uniform"),
            size: std::mem::size_of::<PaperUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        gpu.queue
            .write_buffer(&buf, 0, bytemuck::bytes_of(&uniforms));
        let bind_group = crate::renderer::paper_bind_group(&gpu.device, &bgl, &buf, &lattice);

        let layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("paper-pl"),
                bind_group_layouts: &[Some(&bgl)],
                ..Default::default()
            });
        let pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("paper"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &gpu.shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &gpu.shader,
                    entry_point: Some("fs_paper"),
                    compilation_options: Default::default(),
                    targets: &[Some(format.into())],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });

        let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("desk-target"),
            size: wgpu::Extent3d {
                width: VIEW_W,
                height: VIEW_H,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());

        let row = VIEW_W * 4;
        let padded =
            row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (padded * VIEW_H) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("desk"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                ..Default::default()
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(VIEW_H),
                },
            },
            wgpu::Extent3d {
                width: VIEW_W,
                height: VIEW_H,
                depth_or_array_layers: 1,
            },
        );
        gpu.queue.submit(Some(encoder.finish()));
        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        gpu.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .expect("poll");
        let mapped = slice.get_mapped_range().expect("mapped");
        let mut out = vec![0u8; (row * VIEW_H) as usize];
        for y in 0..VIEW_H as usize {
            let src = y * padded as usize;
            let dst = y * row as usize;
            out[dst..dst + row as usize].copy_from_slice(&mapped[src..src + row as usize]);
        }
        drop(mapped);
        readback.unmap();
        out
    }

    /// The one that matters: trading thirty ALU ops and six branches per pixel for a texel fetch
    /// is only worth anything if the board looks exactly the same afterwards. Byte equality over
    /// a whole viewport, at both backing scales, against the path the lattice was derived from.
    #[test]
    fn the_baked_lattice_draws_the_same_desk_the_shader_computes() {
        let Some(gpu) = crate::test_gpu::gpu() else {
            return;
        };
        for dpr in [1.0f32, 1.5, 2.0] {
            let computed = draw_desk(gpu, dpr, false);
            let baked = draw_desk(gpu, dpr, true);
            assert_eq!(
                computed, baked,
                "the lattice and the procedural path disagree somewhere at {dpr}x"
            );
            assert!(
                computed.chunks(4).any(|p| p[0] != computed[0]),
                "the fixture has to actually draw a grid, or equality is vacuous"
            );
        }
    }

    /// Only `dpr` sizes it, so a second `ensure` at the same backing scale must not rebake —
    /// this is the whole reason the texture is one period instead of a viewport.
    #[test]
    fn rebaking_happens_on_a_backing_scale_change_and_nowhere_else() {
        let Some(gpu) = crate::test_gpu::gpu() else {
            return;
        };
        let mut lattice = DeskLattice::new(&gpu.device, &gpu.queue, 2.0);
        assert_eq!(lattice.shader_side(), 52.0);

        assert!(!lattice.ensure(&gpu.device, &gpu.queue, 2.0));
        assert!(lattice.ensure(&gpu.device, &gpu.queue, 1.0));
        assert_eq!(lattice.shader_side(), 26.0);

        assert!(lattice.ensure(&gpu.device, &gpu.queue, 1.25));
        assert_eq!(
            lattice.shader_side(),
            0.0,
            "zero is what tells the shader to evaluate the pattern itself"
        );
    }
}
