//! The layer table, exercised on a real device.
//!
//! These build the tile and solid pipelines against the *same* `tile_shared_bgl` and the same
//! shader the app uses, so a disagreement between `LayerData` in Rust and `LayerData` in WGSL —
//! a field added on one side, a stride that stopped matching — fails here rather than showing up
//! as geometry in the wrong place on someone's board.

use super::*;
use crate::renderer::pipeline::{premultiplied_target, tile_shared_bgl, TILE_INSTANCE_ATTRS};
use crate::test_gpu::{gpu, read_texture_layer, Gpu};
use calumma_core::tile::{TILE_BYTES, TILE_SIZE};

const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

pub(super) struct Fixture {
    pub(super) bgl: wgpu::BindGroupLayout,
    pub(super) camera: wgpu::Buffer,
    pub(super) layers: wgpu::Buffer,
    pub(super) samplers: TileSamplers,
    pub(super) atlas: TileAtlas,
    pub(super) target: wgpu::Texture,
}

impl Fixture {
    /// A slot in the atlas holding one flat colour, mip chain included so the sampler's
    /// choice of level cannot change what the test reads back.
    fn solid_slot(&mut self, gpu: &Gpu, rgba: [u8; 4]) -> u32 {
        let shared = SharedBindings {
            layout: &self.bgl,
            camera: &self.camera,
            layers: &self.layers,
            samplers: &self.samplers,
        };
        let slot = self
            .atlas
            .allocate(&gpu.device, &gpu.queue, &shared)
            .expect("slot");
        let base = rgba.repeat(TILE_BYTES / 4);
        let mut mips = Vec::new();
        let mut side = TILE_SIZE / 2;
        while side >= 1 {
            mips.push(rgba.repeat((side * side) as usize));
            if side == 1 {
                break;
            }
            side /= 2;
        }
        self.atlas.write(&gpu.queue, slot, &base, &mips);
        slot
    }

    pub(super) fn write_rows(&self, gpu: &Gpu, rows: &[LayerData]) {
        gpu.queue
            .write_buffer(&self.layers, 0, bytemuck::cast_slice(rows));
    }
}

/// One tile's worth of board, drawn 1:1 into a `TILE_SIZE` target: document pixel *n* lands
/// on target pixel *n*, so a readback coordinate is a document coordinate and the mip level
/// is 0 everywhere.
pub(super) fn fixture(gpu: &Gpu) -> Fixture {
    let bgl = tile_shared_bgl(&gpu.device);
    let camera = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("tile-camera"),
        size: std::mem::size_of::<TileCamera>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let side = TILE_SIZE as f32;
    gpu.queue.write_buffer(
        &camera,
        0,
        bytemuck::bytes_of(&TileCamera {
            pan: [0.0, 0.0],
            zoom: 1.0,
            dpr: 1.0,
            viewport: [side, side],
            doc_size: [side, side],
            crisp: 0.0,
            _pad: [0.0; 3],
        }),
    );
    let layers = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("layer-data"),
        size: (LAYER_DATA_CAPACITY * std::mem::size_of::<LayerData>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let samplers = TileSamplers::new(&gpu.device);
    let atlas = TileAtlas::new(
        &gpu.device,
        &SharedBindings {
            layout: &bgl,
            camera: &camera,
            layers: &layers,
            samplers: &samplers,
        },
        8,
    );
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("layer-table-target"),
        size: wgpu::Extent3d {
            width: TILE_SIZE,
            height: TILE_SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TARGET_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    Fixture {
        bgl,
        camera,
        layers,
        samplers,
        atlas,
        target,
    }
}

fn pipeline(gpu: &Gpu, f: &Fixture, vs: &str, fs: &str, instanced: bool) -> wgpu::RenderPipeline {
    let layout = gpu
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("tile-pl"),
            bind_group_layouts: &[Some(&f.bgl)],
            ..Default::default()
        });
    let instance_layout = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<TileInstance>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: TILE_INSTANCE_ATTRS,
    };
    let buffers: &[Option<wgpu::VertexBufferLayout>] = if instanced {
        &[Some(instance_layout)]
    } else {
        &[]
    };
    gpu.device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("layer-table-test"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &gpu.shader,
                entry_point: Some(vs),
                compilation_options: Default::default(),
                buffers,
            },
            fragment: Some(wgpu::FragmentState {
                module: &gpu.shader,
                entry_point: Some(fs),
                compilation_options: Default::default(),
                targets: &[Some(premultiplied_target(TARGET_FORMAT))],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        })
}

/// Runs one draw against a cleared target and hands back the rendered pixels.
fn draw(
    gpu: &Gpu,
    f: &Fixture,
    pipeline: &wgpu::RenderPipeline,
    instances: &[TileInstance],
    range: std::ops::Range<u32>,
) -> Vec<u8> {
    let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("tile-instances"),
        size: ((instances.len().max(1)) * std::mem::size_of::<TileInstance>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    if !instances.is_empty() {
        gpu.queue
            .write_buffer(&buf, 0, bytemuck::cast_slice(instances));
    }
    let view = f
        .target
        .create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("layer-table-pass"),
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
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, f.atlas.bind_group(), &[]);
        if !instances.is_empty() {
            pass.set_vertex_buffer(0, buf.slice(..));
        }
        pass.draw(0..6, range);
    }
    gpu.queue.submit(Some(encoder.finish()));
    read_texture_layer(&gpu.device, &gpu.queue, &f.target, 0, TILE_SIZE)
}

pub(super) fn pixel(image: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * TILE_SIZE + x) * 4) as usize;
    [image[i], image[i + 1], image[i + 2], image[i + 3]]
}

/// The whole point of the table: two tiles in **one** instanced draw, transformed
/// differently, because each instance names its own row. Under the per-layer uniform this
/// needed two draws with a bind group swap between them, and a single draw could only ever
/// place every tile with one transform.
#[test]
fn an_instance_is_transformed_by_the_row_its_layer_index_names() {
    let Some(gpu) = gpu() else { return };
    let mut f = fixture(gpu);
    let red = f.solid_slot(gpu, RED);
    let blue = f.solid_slot(gpu, BLUE);
    let shift = (TILE_SIZE / 2) as f32;
    f.write_rows(
        gpu,
        &[
            LayerData::default(),
            LayerData {
                offset: [shift, 0.0],
                ..LayerData::default()
            },
        ],
    );
    let pipe = pipeline(gpu, &f, "vs_tile", "fs_tile", true);

    let image = draw(
        gpu,
        &f,
        &pipe,
        &[
            TileInstance {
                origin: [0.0, 0.0],
                slot: red,
                layer_index: 0,
            },
            TileInstance {
                origin: [0.0, 0.0],
                slot: blue,
                layer_index: 1,
            },
        ],
        0..2,
    );

    assert_eq!(
        pixel(&image, 8, 8),
        RED,
        "row 0 is identity, so the red tile sits where its origin says"
    );
    assert_eq!(
        pixel(&image, TILE_SIZE - 8, 8),
        BLUE,
        "row 1 offsets by half a tile, so the blue tile covers the right half — same draw, \
         same origin, different row"
    );
}

/// Both tiles carry the *same* origin and the same row; nothing should move. Guards against
/// a shader that reads a row by something other than the index it was handed — an instance
/// counter, say — which the test above alone would not catch.
#[test]
fn two_instances_sharing_a_row_land_in_the_same_place() {
    let Some(gpu) = gpu() else { return };
    let mut f = fixture(gpu);
    let red = f.solid_slot(gpu, RED);
    let blue = f.solid_slot(gpu, BLUE);
    f.write_rows(
        gpu,
        &[
            LayerData::default(),
            LayerData {
                offset: [TILE_SIZE as f32, 0.0],
                ..LayerData::default()
            },
        ],
    );
    let pipe = pipeline(gpu, &f, "vs_tile", "fs_tile", true);

    let image = draw(
        gpu,
        &f,
        &pipe,
        &[
            TileInstance {
                origin: [0.0, 0.0],
                slot: red,
                layer_index: 0,
            },
            TileInstance {
                origin: [0.0, 0.0],
                slot: blue,
                layer_index: 0,
            },
        ],
        0..2,
    );

    assert_eq!(
        pixel(&image, TILE_SIZE - 8, 8),
        BLUE,
        "the second instance read row 0 like the first, so it covers the first everywhere"
    );
    assert_eq!(pixel(&image, 8, 8), BLUE);
}

/// A layer's transform reaches the shader through the row, not through a bind group: scale
/// about the pivot has to survive the move to the table.
#[test]
fn a_rows_scale_and_pivot_still_place_the_tile() {
    let Some(gpu) = gpu() else { return };
    let mut f = fixture(gpu);
    let red = f.solid_slot(gpu, RED);
    let centre = (TILE_SIZE / 2) as f32;
    f.write_rows(
        gpu,
        &[LayerData {
            pivot: [centre, centre],
            scale: [0.5, 0.5],
            ..LayerData::default()
        }],
    );
    let pipe = pipeline(gpu, &f, "vs_tile", "fs_tile", true);

    let image = draw(
        gpu,
        &f,
        &pipe,
        &[TileInstance {
            origin: [0.0, 0.0],
            slot: red,
            layer_index: 0,
        }],
        0..1,
    );

    assert_eq!(
        pixel(&image, centre as u32, centre as u32),
        RED,
        "half scale about the centre keeps the middle covered"
    );
    assert_eq!(
        pixel(&image, 8, 8),
        [0, 0, 0, 0],
        "and pulls the corner in, leaving it clear"
    );
}

/// Solid Paper has no instance buffer to carry an atlas slot, so its row holds one and the
/// draw names the row through its instance range. This is what replaced bitcasting the slot
/// into `pivot.x` — two draw paths reading the same bytes as different types.
#[test]
fn the_solid_quad_reads_its_atlas_slot_from_the_row_the_draw_range_names() {
    let Some(gpu) = gpu() else { return };
    let mut f = fixture(gpu);
    let red = f.solid_slot(gpu, RED);
    let blue = f.solid_slot(gpu, BLUE);
    f.write_rows(
        gpu,
        &[
            LayerData {
                atlas_slot: red,
                ..LayerData::default()
            },
            LayerData {
                atlas_slot: blue,
                ..LayerData::default()
            },
        ],
    );
    let pipe = pipeline(gpu, &f, "vs_doc_quad", "fs_solid_tile", false);

    let image = draw(gpu, &f, &pipe, &[], 1..2);

    assert_eq!(
        pixel(&image, 8, 8),
        BLUE,
        "instance range 1..2 selects row 1, whose atlas slot is the blue tile"
    );
    assert_eq!(pixel(&image, TILE_SIZE - 8, TILE_SIZE - 8), BLUE);
}

/// The Rust row and the WGSL row have to agree byte for byte, and nothing in the type system
/// enforces it. 1072 bytes is also what makes the WGSL array stride 1072 with no tail
/// padding (the struct's own alignment is 8, from the three `vec2<f32>` fields, and 1072 is
/// already a multiple of 8) — a mismatch here misaddresses every row past the first. Plan 23
/// grew this from 32 to 1072 deliberately; see `LayerData`'s own doc comment for the layout.
#[test]
fn a_table_row_is_the_size_the_shader_strides_by() {
    assert_eq!(std::mem::size_of::<LayerData>(), 1080);
    assert_eq!(std::mem::align_of::<LayerData>(), 4);
    assert_eq!(
        std::mem::size_of::<TileInstance>(),
        16,
        "layer_index took the place of padding, so instances did not grow"
    );
}
