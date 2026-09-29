//! The overview's level pyramid: which half-scale levels exist, allocating them under the GPU
//! budget, and writing a document rectangle into one.

use super::*;

pub(super) struct Level {
    pub(super) max_side: u32,
    pub(super) tex_width: u32,
    pub(super) tex_height: u32,
    pub(super) texture: Option<wgpu::Texture>,
    pub(super) bind_group: Option<wgpu::BindGroup>,
    pub(super) full_dirty: bool,
    pub(super) dirty_chunks: FxHashSet<(i32, i32)>,
}

pub(super) fn write_rect(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    rgba: &[u8],
) {
    if w == 0 || h == 0 || rgba.is_empty() {
        return;
    }
    let row = w * 4;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded = row.div_ceil(align) * align;
    let layout = wgpu::TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(padded),
        rows_per_image: Some(h),
    };
    let dest = wgpu::TexelCopyTextureInfo {
        texture,
        mip_level: 0,
        origin: wgpu::Origin3d { x, y, z: 0 },
        aspect: wgpu::TextureAspect::All,
    };
    let size = wgpu::Extent3d {
        width: w,
        height: h,
        depth_or_array_layers: 1,
    };
    if padded == row {
        queue.write_texture(dest, rgba, layout, size);
        return;
    }
    let mut packed = vec![0u8; padded as usize * h as usize];
    for y in 0..h as usize {
        let src = y * row as usize;
        let dst = y * padded as usize;
        packed[dst..dst + row as usize].copy_from_slice(&rgba[src..src + row as usize]);
    }
    queue.write_texture(dest, &packed, layout, size);
}

impl OverviewPass {
    pub(super) fn ensure_pyramid(&mut self, doc: &Document, budget: &GpuBudget) {
        let dw = doc.width;
        let dh = doc.height;
        let sides = pyramid_sides(dw, dh, budget.overview_finest_side());
        let same = self.levels.len() == sides.len()
            && self.levels.iter().zip(&sides).all(|(level, &side)| {
                level.max_side == side && self.doc_width == dw && self.doc_height == dh
            });
        if same {
            return;
        }
        self.levels = sides
            .into_iter()
            .map(|max_side| {
                let (tex_width, tex_height) = Document::overview_dimensions(dw, dh, max_side);
                Level {
                    max_side,
                    tex_width,
                    tex_height,
                    texture: None,
                    bind_group: None,
                    full_dirty: true,
                    dirty_chunks: FxHashSet::default(),
                }
            })
            .collect();
        self.displayed = 0;
        self.stamp = 0;
    }

    pub(super) fn ensure_level(
        &mut self,
        index: usize,
        doc: &Document,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let Some(level) = self.levels.get(index) else {
            return;
        };
        let max_side = level.max_side;
        let tw = level.tex_width.max(1);
        let th = level.tex_height.max(1);
        let missing = level.texture.is_none();
        let full = missing || level.full_dirty;
        let chunks: Vec<(i32, i32)> = if full {
            Vec::new()
        } else {
            level.dirty_chunks.iter().copied().collect()
        };
        if !full && chunks.is_empty() {
            return;
        }
        if full {
            let rgba = doc.composite_overview_rect(max_side, 0, 0, tw, th);
            if missing {
                self.allocate_level(index, device, tw, th, &rgba, queue);
            } else if let Some(level) = self.levels.get_mut(index) {
                if let Some(texture) = level.texture.as_ref() {
                    write_rect(queue, texture, 0, 0, tw, th, &rgba);
                }
                level.full_dirty = false;
                level.dirty_chunks.clear();
            }
            return;
        }
        let dw = doc.width.max(1);
        let dh = doc.height.max(1);
        if let Some(level) = self.levels.get_mut(index) {
            if let Some(texture) = level.texture.as_ref() {
                for (cx, cy) in chunks {
                    let Some((x, y, w, h)) = chunk_tex_rect(cx, cy, dw, dh, tw, th) else {
                        continue;
                    };
                    let rgba = doc.composite_overview_rect(max_side, x, y, w, h);
                    write_rect(queue, texture, x, y, w, h, &rgba);
                }
            }
            level.dirty_chunks.clear();
        }
    }

    pub(super) fn allocate_level(
        &mut self,
        index: usize,
        device: &wgpu::Device,
        tw: u32,
        th: u32,
        rgba: &[u8],
        queue: &wgpu::Queue,
    ) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("overview"),
            size: wgpu::Extent3d {
                width: tw,
                height: th,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        write_rect(queue, &texture, 0, 0, tw, th, rgba);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("overview-bg"),
            layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.camera_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        if let Some(level) = self.levels.get_mut(index) {
            level.texture = Some(texture);
            level.bind_group = Some(bind_group);
            level.full_dirty = false;
            level.dirty_chunks.clear();
        }
        self.allocations += 1;
    }
}
