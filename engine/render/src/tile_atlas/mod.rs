mod bindings;
#[cfg(test)]
mod tests;

pub use bindings::{SharedBindings, TileSamplers};

use bindings::build_bind_group;
use calumma_core::limits::TILE_ATLAS_INITIAL_CAPACITY;
use calumma_core::tile::{TILE_BYTES, TILE_SIZE};
use std::collections::{HashMap, HashSet};

/// Full mip chain depth for a square, power-of-two tile: 256 → 128 → … → 1. Shared with
/// `compose::tile_mip_chain`, which is what actually builds the per-level pixel data — this
/// only has to agree on *how many* levels exist.
pub(crate) fn tile_mip_levels() -> u32 {
    TILE_SIZE.ilog2() + 1
}

/// One shared `texture_2d_array` holding every GPU-resident tile across the whole document —
/// every layer, pooled together, addressed by array-layer index. The point is draw calls:
/// with one texture per tile, every visible tile would need its own bind group
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
pub struct TileAtlas {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    capacity: u32,
    max_capacity: u32,
    free: Vec<u32>,
}

impl TileAtlas {
    pub fn new(device: &wgpu::Device, shared: &SharedBindings, max_capacity: u32) -> Self {
        let max_capacity = max_capacity.max(1);
        let capacity = TILE_ATLAS_INITIAL_CAPACITY.min(max_capacity);
        let texture = create_array_texture(device, capacity);
        let bind_group = build_bind_group(device, shared, &texture);
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

    /// Rebinds group 0 against the same texture. The atlas does this itself when it grows; this
    /// is for the other half of the descriptor — the layer table, which is reallocated by a
    /// document with more layers than the buffer had room for. A bind group holds the buffer it
    /// was built from, so without this the shader would keep reading the freed one.
    pub fn rebuild_bind_group(&mut self, device: &wgpu::Device, shared: &SharedBindings) {
        self.bind_group = build_bind_group(device, shared, &self.texture);
    }

    /// VRAM reserved by the atlas's declared capacity, mip chain included — every layer costs
    /// its base `TILE_BYTES` plus the smaller levels underneath it, not just the base level.
    pub fn capacity_bytes(&self) -> usize {
        self.capacity as usize * bytes_per_slot()
    }

    /// Sets the ceiling `allocate` grows towards. Lowering it only stops *future* growth — a
    /// texture already bigger than the new ceiling keeps its current capacity until
    /// [`Self::shrink_to`] actually recreates it smaller.
    pub fn set_max_capacity(&mut self, max_capacity: u32) {
        self.max_capacity = max_capacity.max(1);
    }

    /// Recreates the texture at `target_capacity` — clamped up to however many slots are
    /// actually occupied, so a live tile is never dropped by this alone — compacting every
    /// occupied slot into the low indices of the new array in ascending order. Returns the
    /// remap from old array-layer index to new for every slot that moved, so the caller can
    /// update whatever it keys by array layer; a no-op (already at or under target) returns an
    /// empty map and touches nothing.
    ///
    /// This is a full recreate-and-blit, the same cost as [`Self::grow`] — call it only once
    /// pressure has actually persisted (`calumma_core::PressureState`), never on a single
    /// transient signal.
    pub fn shrink_to(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        shared: &SharedBindings,
        target_capacity: u32,
    ) -> HashMap<u32, u32> {
        let free: HashSet<u32> = self.free.iter().copied().collect();
        let occupied: Vec<u32> = (0..self.capacity).filter(|s| !free.contains(s)).collect();
        let target_capacity = target_capacity.max(occupied.len() as u32).max(1);
        if target_capacity >= self.capacity {
            return HashMap::new();
        }

        let texture = create_array_texture(device, target_capacity);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("tile-atlas-shrink"),
        });
        let mut remap = HashMap::with_capacity(occupied.len());
        for (new_index, &old_index) in occupied.iter().enumerate() {
            let new_index = new_index as u32;
            if new_index != old_index {
                remap.insert(old_index, new_index);
            }
            let mut side = TILE_SIZE;
            for level in 0..tile_mip_levels() {
                encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &self.texture,
                        mip_level: level,
                        origin: wgpu::Origin3d {
                            x: 0,
                            y: 0,
                            z: old_index,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: level,
                        origin: wgpu::Origin3d {
                            x: 0,
                            y: 0,
                            z: new_index,
                        },
                        aspect: wgpu::TextureAspect::All,
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
        queue.submit(Some(encoder.finish()));

        self.bind_group = build_bind_group(device, shared, &texture);
        self.texture = texture;
        self.free = (occupied.len() as u32..target_capacity).rev().collect();
        self.capacity = target_capacity;
        remap
    }

    /// A free array-layer index, growing the atlas first if every slot is taken and there is
    /// still room under `max_capacity`. `None` means the atlas is completely full — the caller
    /// must free a slot (evicting some other tile) before this can succeed.
    pub fn allocate(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        shared: &SharedBindings,
    ) -> Option<u32> {
        if self.free.is_empty() && self.capacity < self.max_capacity {
            self.grow(device, queue, shared);
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
    /// tile's own `Arc<Vec<u8>>` — an unclipped layer needs no bake,
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
    /// recomposite (clip baking), so growth preserves them rather than forcing
    /// every live tile to re-upload, which would show up as a stutter at exactly the moment
    /// — many tiles already live — growth tends to happen.
    fn grow(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, shared: &SharedBindings) {
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

        self.bind_group = build_bind_group(device, shared, &texture);
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
