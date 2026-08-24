use calumma_core::limits::TILE_ATLAS_INITIAL_CAPACITY;
use calumma_core::tile::{TILE_BYTES, TILE_SIZE};

/// Full mip chain depth for a square, power-of-two tile: 256 → 128 → … → 1. Shared with
/// `compose::tile_mip_chain`, which is what actually builds the per-level pixel data — this
/// only has to agree on *how many* levels exist.
pub(crate) fn tile_mip_levels() -> u32 {
    TILE_SIZE.ilog2() + 1
}

/// One shared `texture_2d_array` holding every GPU-resident tile across the whole document —
/// every layer, pooled together, addressed by array-layer index. The point is draw calls:
/// with one texture per tile (the old design), every visible tile needed its own bind group
/// and its own `draw()`. With a shared array, a whole document layer's tiles become a single
/// instanced draw — the array bound once, a per-tile origin and array-layer index riding in
/// an instance buffer. On a large, multi-layer, zoomed-out document, or on a weak integrated
/// GPU where per-draw-call overhead dominates, that is the difference between a few dozen
/// draw calls a frame and several thousand.
///
/// Grown (never shrunk) on demand, doubling from [`TILE_ATLAS_INITIAL_CAPACITY`] up to
/// `max_capacity` — so a small document never pays VRAM for a big array, since a `wgpu::Texture`
/// reserves storage for its whole declared layer count regardless of how much is written. Once
/// `max_capacity` is reached, [`TileAtlas::allocate`] returns `None` and the caller — `sync_tiles`
/// in `renderer.rs` — is responsible for freeing a slot first, which in practice means evicting
/// a prefetch-margin tile (one retained just outside the viewport) before ever giving up
/// something the viewport can actually see.
///
/// Every layer carries a full mip chain (see `compose::tile_mip_chain`), so panning or zooming
/// out samples pre-filtered, smaller mips instead of raw 256×256 texels through a plain bilinear
/// filter — which is what shimmering/moiré during a pan actually is: minification aliasing from
/// sampling a texture well below its native resolution with no mips to fall back to. The chain
/// adds roughly a third more storage on top of the base level (256×256 → ~1.33×), so the atlas's
/// real worst case is closer to 1.3GiB than the 1GiB `TILE_ATLAS_MAX_CAPACITY * TILE_BYTES`
/// alone would suggest — accounted for in [`TileAtlas::capacity_bytes`].
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

pub struct TileAtlas {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    capacity: u32,
    max_capacity: u32,
    free: Vec<u32>,
}

impl TileAtlas {
    pub fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        camera_buf: &wgpu::Buffer,
        samplers: &TileSamplers,
        max_capacity: u32,
    ) -> Self {
        let max_capacity = max_capacity.max(1);
        let capacity = TILE_ATLAS_INITIAL_CAPACITY.min(max_capacity);
        let texture = create_array_texture(device, capacity);
        let bind_group = build_bind_group(device, layout, camera_buf, &texture, samplers);
        Self {
            texture,
            bind_group,
            capacity,
            max_capacity,
            free: (0..capacity).rev().collect(),
        }
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    /// VRAM reserved by the atlas's declared capacity, mip chain included — every layer costs
    /// its base `TILE_BYTES` plus the smaller levels underneath it, not just the base level.
    pub fn capacity_bytes(&self) -> usize {
        self.capacity as usize * bytes_per_slot()
    }

    /// A free array-layer index, growing the atlas first if every slot is taken and there is
    /// still room under `max_capacity`. `None` means the atlas is completely full — the caller
    /// must free a slot (evicting some other tile) before this can succeed.
    pub fn allocate(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        camera_buf: &wgpu::Buffer,
        samplers: &TileSamplers,
    ) -> Option<u32> {
        if self.free.is_empty() && self.capacity < self.max_capacity {
            self.grow(device, queue, layout, camera_buf, samplers);
        }
        self.free.pop()
    }

    pub fn free(&mut self, slot: u32) {
        self.free.push(slot);
    }

    /// Releases every slot at once for a closed document. The texture itself is kept — it is
    /// worth reusing for whatever project opens next rather than paying to reallocate it.
    pub fn clear(&mut self) {
        self.free = (0..self.capacity).rev().collect();
    }

    /// Writes a whole mip chain for one tile — `levels[0]` is the base 256×256 image, each
    /// entry after it half the size of the one before, as built by `compose::tile_mip_chain`.
    /// Base level and mip chain are passed separately because the base is very often the
    /// tile's own `Arc<Vec<u8>>` — an unmasked, unadjusted, fully opaque layer needs no bake,
    /// and `queue.write_texture` can read those bytes where they already live. Folding it into
    /// the level vector meant a 256 KiB allocation and memcpy per tile per upload for pixels
    /// that were about to be copied again anyway.
    pub fn write(&self, queue: &wgpu::Queue, slot: u32, base: &[u8], mips: &[Vec<u8>]) {
        let mut side = TILE_SIZE;
        for (level, data) in std::iter::once(base)
            .chain(mips.iter().map(Vec::as_slice))
            .enumerate()
        {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: slot,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(side * 4),
                    rows_per_image: Some(side),
                },
                wgpu::Extent3d {
                    width: side,
                    height: side,
                    depth_or_array_layers: 1,
                },
            );
            side = (side / 2).max(1);
        }
    }

    /// Doubles capacity (capped at `max_capacity`) into a fresh texture and copies every
    /// existing layer across with a GPU-side blit — tile pixels can be expensive to
    /// recomposite (mask/adjustment baking), so growth preserves them rather than forcing
    /// every live tile to re-upload, which would show up as a stutter at exactly the moment
    /// — many tiles already live — growth tends to happen.
    fn grow(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        camera_buf: &wgpu::Buffer,
        samplers: &TileSamplers,
    ) {
        let old_capacity = self.capacity;
        let next = (old_capacity.saturating_mul(2))
            .min(self.max_capacity)
            .max(old_capacity + 1);
        let texture = create_array_texture(device, next);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("tile-atlas-grow"),
        });
        let mut side = TILE_SIZE;
        for level in 0..tile_mip_levels() {
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: level,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: side,
                    height: side,
                    depth_or_array_layers: old_capacity,
                },
            );
            side = (side / 2).max(1);
        }
        queue.submit(Some(encoder.finish()));

        self.bind_group = build_bind_group(device, layout, camera_buf, &texture, samplers);
        self.texture = texture;
        self.free.extend(old_capacity..next);
        self.capacity = next;
    }
}

fn create_array_texture(device: &wgpu::Device, capacity: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("tile-atlas"),
        size: wgpu::Extent3d {
            width: TILE_SIZE,
            height: TILE_SIZE,
            depth_or_array_layers: capacity,
        },
        mip_level_count: tile_mip_levels(),
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// Bytes one atlas slot costs including its full mip chain: `TILE_BYTES` at the base level,
/// plus each halving contributing roughly a quarter of the level before it — summing to about
/// `4/3` of the base size for a square power-of-two texture (256×256: 349,524 bytes vs. a base
/// of 262,144).
fn bytes_per_slot() -> usize {
    let mut total = 0usize;
    let mut side = TILE_SIZE as usize;
    for _ in 0..tile_mip_levels() {
        total += side * side * 4;
        side = (side / 2).max(1);
    }
    debug_assert!(
        total > TILE_BYTES,
        "mip chain should add strictly more than the base level"
    );
    total
}

fn build_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    camera_buf: &wgpu::Buffer,
    texture: &wgpu::Texture,
    samplers: &TileSamplers,
) -> wgpu::BindGroup {
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("tile-atlas-bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&samplers.smooth),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(&samplers.crisp),
            },
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_gpu::{gpu, read_texture_layer, Gpu};
    use calumma_core::limits::TILE_ATLAS_INITIAL_CAPACITY;

    struct Fixture {
        layout: wgpu::BindGroupLayout,
        camera_buf: wgpu::Buffer,
        samplers: TileSamplers,
    }

    /// The same Group 0 the renderer binds: camera uniform, the tile array, and the two
    /// samplers. Built here rather than borrowed from `renderer.rs` so a layout change there
    /// fails these tests loudly instead of silently drifting.
    fn fixture(gpu: &Gpu) -> Fixture {
        let layout = gpu
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("tile-shared-bgl"),
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
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let camera_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tile-camera"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Fixture {
            layout,
            camera_buf,
            samplers: TileSamplers::new(&gpu.device),
        }
    }

    fn atlas(gpu: &Gpu, f: &Fixture, max_capacity: u32) -> TileAtlas {
        TileAtlas::new(
            &gpu.device,
            &f.layout,
            &f.camera_buf,
            &f.samplers,
            max_capacity,
        )
    }

    fn allocate(gpu: &Gpu, f: &Fixture, atlas: &mut TileAtlas) -> Option<u32> {
        atlas.allocate(
            &gpu.device,
            &gpu.queue,
            &f.layout,
            &f.camera_buf,
            &f.samplers,
        )
    }

    #[test]
    fn a_fresh_atlas_hands_out_every_slot_once_and_then_refuses() {
        let Some(gpu) = gpu() else { return };
        let f = fixture(gpu);
        let mut atlas = atlas(gpu, &f, 4);

        let slots: Vec<u32> = (0..4)
            .map(|_| allocate(gpu, &f, &mut atlas).expect("slot"))
            .collect();
        assert_eq!(slots, vec![0, 1, 2, 3], "slots come out in index order");
        assert_eq!(
            allocate(gpu, &f, &mut atlas),
            None,
            "a full atlas at max capacity cannot grow, and says so"
        );
    }

    /// The free list is a LIFO `Vec<u32>` on purpose (`docs/plans/02`): the slot released last is
    /// the slot handed out next, so a steady state of evict-then-upload never fragments.
    #[test]
    fn a_freed_slot_is_the_next_one_handed_out() {
        let Some(gpu) = gpu() else { return };
        let f = fixture(gpu);
        let mut atlas = atlas(gpu, &f, 4);
        for _ in 0..4 {
            allocate(gpu, &f, &mut atlas).expect("slot");
        }

        atlas.free(2);
        assert_eq!(allocate(gpu, &f, &mut atlas), Some(2));
        assert_eq!(allocate(gpu, &f, &mut atlas), None);
    }

    #[test]
    fn clear_releases_every_slot_without_reallocating_the_texture() {
        let Some(gpu) = gpu() else { return };
        let f = fixture(gpu);
        let mut atlas = atlas(gpu, &f, 4);
        for _ in 0..4 {
            allocate(gpu, &f, &mut atlas).expect("slot");
        }
        let before = atlas.capacity_bytes();

        atlas.clear();

        assert_eq!(atlas.capacity_bytes(), before, "capacity is kept for reuse");
        let slots: Vec<u32> = (0..4)
            .map(|_| allocate(gpu, &f, &mut atlas).expect("slot"))
            .collect();
        assert_eq!(slots, vec![0, 1, 2, 3]);
    }

    #[test]
    fn max_capacity_is_clamped_to_at_least_one_slot() {
        let Some(gpu) = gpu() else { return };
        let f = fixture(gpu);
        let mut atlas = atlas(gpu, &f, 0);

        assert_eq!(allocate(gpu, &f, &mut atlas), Some(0));
        assert_eq!(allocate(gpu, &f, &mut atlas), None);
    }

    /// A slot costs its base tile *plus* its mip chain — roughly 4/3 of `TILE_BYTES`, which is
    /// the number `renderer.rs` budgets VRAM against.
    #[test]
    fn capacity_bytes_counts_the_mip_chain_not_just_the_base_level() {
        let Some(gpu) = gpu() else { return };
        let f = fixture(gpu);
        let atlas = atlas(gpu, &f, 4);

        assert_eq!(atlas.capacity_bytes(), 4 * bytes_per_slot());
        assert!(bytes_per_slot() > TILE_BYTES);
        assert!(bytes_per_slot() < TILE_BYTES * 3 / 2);
        assert_eq!(tile_mip_levels(), 9, "256 -> 128 -> ... -> 1");
    }

    #[test]
    fn running_out_of_slots_doubles_the_atlas_instead_of_failing() {
        let Some(gpu) = gpu() else { return };
        let f = fixture(gpu);
        let mut atlas = atlas(gpu, &f, TILE_ATLAS_INITIAL_CAPACITY * 2);
        let initial_bytes = atlas.capacity_bytes();

        for _ in 0..TILE_ATLAS_INITIAL_CAPACITY {
            allocate(gpu, &f, &mut atlas).expect("slot");
        }
        let extra = allocate(gpu, &f, &mut atlas).expect("growth gives one more");

        assert!(extra >= TILE_ATLAS_INITIAL_CAPACITY, "a brand new slot");
        assert_eq!(atlas.capacity_bytes(), initial_bytes * 2);
    }

    /// Growth is a blit, not a re-upload: tile pixels can be expensive to recomposite, so the
    /// pixels already on the GPU have to survive the move to the larger texture.
    #[test]
    fn growth_carries_the_pixels_already_in_the_atlas_across() {
        let Some(gpu) = gpu() else { return };
        let f = fixture(gpu);
        let mut atlas = atlas(gpu, &f, TILE_ATLAS_INITIAL_CAPACITY * 2);

        let slot = allocate(gpu, &f, &mut atlas).expect("slot");
        let mut base = vec![0u8; TILE_BYTES];
        base[0..4].copy_from_slice(&[10, 20, 30, 255]);
        let last = TILE_BYTES - 4;
        base[last..].copy_from_slice(&[40, 50, 60, 255]);
        atlas.write(&gpu.queue, slot, &base, &[]);

        for _ in 1..=TILE_ATLAS_INITIAL_CAPACITY {
            allocate(gpu, &f, &mut atlas).expect("slot");
        }

        let read = read_texture_layer(&gpu.device, &gpu.queue, &atlas.texture, slot, TILE_SIZE);
        assert_eq!(&read[0..4], &[10, 20, 30, 255]);
        assert_eq!(&read[last..], &[40, 50, 60, 255]);
    }
}
