/// The two samplers every atlas bind group carries, differing only in `mag_filter`. Which one
/// `fs_tile` reads is a per-frame decision on `TileCamera::crisp` rather than a rebind: past
/// `limits::CRISP_PIXEL_ZOOM` the board is magnifying, and a bilinear tap turns one texel into
/// a gradient the width of the whole magnified pixel — the opposite of what zooming that far
/// in is for. Everything at or below 1:1 is a minification, which wants filtering and the mip
/// chain, so both samplers keep `min_filter`/`mipmap_filter` linear.
pub struct TileSamplers {
    smooth: wgpu::Sampler,
    crisp: wgpu::Sampler,
}

impl TileSamplers {
    pub fn new(device: &wgpu::Device) -> Self {
        let descriptor = |label: &'static str, mag_filter: wgpu::FilterMode| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some(label),
                mag_filter,
                min_filter: wgpu::FilterMode::Linear,
                // Tiles carry a full mip chain (`TileAtlas`/`compose::tile_mip_chain`) precisely
                // so this can blend between levels: `fs_tile` samples with automatic LOD, and
                // without this the GPU would still pick the right mip but snap to it instead of
                // blending, which shows up as visible seams sweeping across the board while
                // zooming.
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            })
        };
        Self {
            smooth: descriptor("tile-sampler", wgpu::FilterMode::Linear),
            crisp: descriptor("tile-sampler-crisp", wgpu::FilterMode::Nearest),
        }
    }
}

/// Everything bind group 0 is built from: the layout, the per-frame camera uniform, the layer
/// table the tile shader indexes per instance, and the two samplers.
///
/// Bundled rather than passed as four parameters because the atlas rebuilds this bind group
/// every time it grows, and the layer table rebuilds it every time the buffer is reallocated —
/// two independent lifetimes writing the same descriptor. One struct keeps them in step: add a
/// binding here and every rebuild path gets it, instead of one of them silently keeping a stale
/// resource. Callers construct it from disjoint `Renderer` fields, so it borrows fine alongside
/// `&mut self.atlas`.
pub struct SharedBindings<'a> {
    pub layout: &'a wgpu::BindGroupLayout,
    pub camera: &'a wgpu::Buffer,
    pub layers: &'a wgpu::Buffer,
    pub samplers: &'a TileSamplers,
}

pub(super) fn build_bind_group(
    device: &wgpu::Device,
    shared: &SharedBindings,
    texture: &wgpu::Texture,
) -> wgpu::BindGroup {
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("tile-atlas-bg"),
        layout: shared.layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: shared.camera.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&shared.samplers.smooth),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(&shared.samplers.crisp),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: shared.layers.as_entire_binding(),
            },
        ],
    })
}
