//! One headless device, shared by this crate's GPU-backed unit tests.
//!
//! The atlas, the pan cache and the overview pass all need a real `wgpu::Device` to exist at
//! all — they are a texture array, two color targets and a pipeline — but none of them needs a
//! *surface*, so their tests run with no window. Adapter and device creation is the slow part
//! and the shader is compiled once, so both are done once per test binary rather than once per
//! test.

use std::sync::OnceLock;

pub(crate) struct Gpu {
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) shader: wgpu::ShaderModule,
}

/// `None` where no adapter can be had; callers return instead of failing, the same bargain
/// `stroke_coverage`'s own harness makes.
pub(crate) fn gpu() -> Option<&'static Gpu> {
    static GPU: OnceLock<Option<Gpu>> = OnceLock::new();
    GPU.get_or_init(|| {
        let instance = wgpu::Instance::default();
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .ok()?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("board"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/board.wgsl").into()),
        });
        Some(Gpu {
            device,
            queue,
            shader,
        })
    })
    .as_ref()
}

/// Reads one whole 256×256 mip level back off the GPU. Tile rows are 1024 bytes, which is
/// already a multiple of `COPY_BYTES_PER_ROW_ALIGNMENT`, so no padding stride is involved.
pub(crate) fn read_texture_layer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    layer: u32,
    side: u32,
) -> Vec<u8> {
    let bytes = (side * side * 4) as u64;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: bytes,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("readback"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: 0,
                y: 0,
                z: layer,
            },
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(side * 4),
                rows_per_image: Some(side),
            },
        },
        wgpu::Extent3d {
            width: side,
            height: side,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .expect("poll");
    let mapped = slice.get_mapped_range().expect("mapped");
    let out = mapped.to_vec();
    drop(mapped);
    buffer.unmap();
    out
}
