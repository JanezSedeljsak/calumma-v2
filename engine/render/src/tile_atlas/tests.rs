use super::*;
use crate::test_gpu::{gpu, read_texture_layer, Gpu};
use calumma_core::limits::TILE_ATLAS_INITIAL_CAPACITY;

struct Fixture {
    layout: wgpu::BindGroupLayout,
    camera_buf: wgpu::Buffer,
    layer_buf: wgpu::Buffer,
    samplers: TileSamplers,
}

impl Fixture {
    fn shared(&self) -> SharedBindings<'_> {
        SharedBindings {
            layout: &self.layout,
            camera: &self.camera_buf,
            layers: &self.layer_buf,
            samplers: &self.samplers,
        }
    }
}

/// The same Group 0 the renderer binds: camera uniform, the tile array, the two samplers
/// and the layer table. Built here rather than borrowed from `renderer.rs` so a layout
/// change there fails these tests loudly instead of silently drifting.
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
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
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
    let layer_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("layer-data"),
        size: 32,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    Fixture {
        layout,
        camera_buf,
        layer_buf,
        samplers: TileSamplers::new(&gpu.device),
    }
}

fn atlas(gpu: &Gpu, f: &Fixture, max_capacity: u32) -> TileAtlas {
    TileAtlas::new(&gpu.device, &f.shared(), max_capacity)
}

fn allocate(gpu: &Gpu, f: &Fixture, atlas: &mut TileAtlas) -> Option<u32> {
    atlas.allocate(&gpu.device, &gpu.queue, &f.shared())
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

/// The free list is a LIFO `Vec<u32>` on purpose: the slot released last is
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

#[test]
fn lowering_max_capacity_stops_growth_without_touching_current_capacity() {
    let Some(gpu) = gpu() else { return };
    let f = fixture(gpu);
    let mut atlas = atlas(gpu, &f, TILE_ATLAS_INITIAL_CAPACITY * 2);
    let before = atlas.capacity_bytes();

    for _ in 0..TILE_ATLAS_INITIAL_CAPACITY {
        allocate(gpu, &f, &mut atlas).expect("one of the initial slots");
    }
    assert_eq!(atlas.capacity_bytes(), before, "no growth needed yet");

    // Lower the ceiling to exactly the capacity already granted: nothing to reclaim (that
    // is `shrink_to`'s job), but no further growth either.
    atlas.set_max_capacity(TILE_ATLAS_INITIAL_CAPACITY);
    assert_eq!(
        atlas.capacity_bytes(),
        before,
        "existing texture is untouched"
    );
    assert_eq!(
        allocate(gpu, &f, &mut atlas),
        None,
        "the lowered ceiling refuses growth even though doubling would otherwise succeed"
    );
}

#[test]
fn shrink_to_a_capacity_still_above_target_is_a_no_op() {
    let Some(gpu) = gpu() else { return };
    let f = fixture(gpu);
    let mut atlas = atlas(gpu, &f, 4);
    let before = atlas.capacity_bytes();

    let remap = atlas.shrink_to(&gpu.device, &gpu.queue, &f.shared(), 8);

    assert!(remap.is_empty());
    assert_eq!(atlas.capacity_bytes(), before);
}

#[test]
fn shrink_to_never_drops_below_what_is_actually_occupied() {
    let Some(gpu) = gpu() else { return };
    let f = fixture(gpu);
    let mut atlas = atlas(gpu, &f, 8);
    for _ in 0..6 {
        allocate(gpu, &f, &mut atlas).expect("slot");
    }

    atlas.shrink_to(&gpu.device, &gpu.queue, &f.shared(), 2);

    assert_eq!(
        atlas.capacity_bytes(),
        6 * bytes_per_slot(),
        "clamped up to the 6 slots still in use"
    );
    // `shrink_to` alone doesn't lower the ceiling — pair it with `set_max_capacity` the way
    // the renderer does, or the next `allocate` just grows back into the room under it.
    atlas.set_max_capacity(2);
    assert_eq!(
        allocate(gpu, &f, &mut atlas),
        None,
        "every slot is occupied"
    );
}

/// Shrinking compacts occupied slots into the low indices of the new, smaller texture and
/// carries their pixels across — the whole point is reclaiming VRAM without losing what is
/// still on screen.
#[test]
fn shrink_to_compacts_occupied_slots_and_carries_pixels() {
    let Some(gpu) = gpu() else { return };
    let f = fixture(gpu);
    let mut atlas = atlas(gpu, &f, 8);
    let slots: Vec<u32> = (0..4)
        .map(|_| allocate(gpu, &f, &mut atlas).expect("slot"))
        .collect();
    assert_eq!(slots, vec![0, 1, 2, 3]);

    // Free the middle two, leaving a fragmented occupied set: {0, 3}.
    atlas.free(1);
    atlas.free(2);

    let mut base0 = vec![0u8; TILE_BYTES];
    base0[0..4].copy_from_slice(&[1, 2, 3, 255]);
    atlas.write(&gpu.queue, 0, &base0, &[]);
    let mut base3 = vec![0u8; TILE_BYTES];
    base3[0..4].copy_from_slice(&[9, 8, 7, 255]);
    atlas.write(&gpu.queue, 3, &base3, &[]);

    let remap = atlas.shrink_to(&gpu.device, &gpu.queue, &f.shared(), 2);

    assert_eq!(atlas.capacity_bytes(), 2 * bytes_per_slot());
    assert_eq!(remap.get(&3), Some(&1), "slot 3 compacts down to index 1");
    assert!(
        !remap.contains_key(&0),
        "slot 0 already sat at its target index"
    );

    let read0 = read_texture_layer(&gpu.device, &gpu.queue, &atlas.texture, 0, TILE_SIZE);
    assert_eq!(&read0[0..4], &[1, 2, 3, 255]);
    let read1 = read_texture_layer(&gpu.device, &gpu.queue, &atlas.texture, 1, TILE_SIZE);
    assert_eq!(&read1[0..4], &[9, 8, 7, 255]);

    atlas.set_max_capacity(2);
    assert_eq!(
        allocate(gpu, &f, &mut atlas),
        None,
        "both compacted slots are occupied"
    );
}
