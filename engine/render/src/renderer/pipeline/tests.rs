//! Everything a `Renderer` does needs a `wgpu::Surface`, which needs a window, so what can be
//! tested here without one is what the renderer *decides* rather than what it draws: the blend
//! state each layer blend mode compiles to, the bind-group and vertex layouts the pipelines are
//! declared with, and the tile spans that drive upload and eviction. The drawing itself is
//! covered where it can be — `stroke_coverage`, `tile_atlas`, `framebuffer` and `overview` all
//! run against a headless device.

use super::*;

/// The blend pipelines only exist on a real surface-backed `Renderer`, so a mismatch
/// between `fs_tile_blend`'s bindings and the two layouts would otherwise first surface as a
/// validation panic at app start. Building them here against the same layouts catches it.
#[test]
fn the_blend_pipelines_validate_against_the_shared_and_backdrop_layouts() {
    let Some(gpu) = crate::test_gpu::gpu() else {
        return;
    };
    let shared = tile_shared_bgl(&gpu.device);
    let backdrop = backdrop_bgl(&gpu.device);
    let layout = gpu
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blend-test-pl"),
            bind_group_layouts: &[Some(&shared), Some(&backdrop)],
            ..Default::default()
        });
    let instance_layout = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<TileInstance>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: TILE_INSTANCE_ATTRS,
    };
    for (vs, fs, buffers) in [
        ("vs_tile", "fs_tile_blend", vec![Some(instance_layout)]),
        ("vs_doc_quad", "fs_solid_tile_blend", vec![]),
    ] {
        let _pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(fs),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &gpu.shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers: &buffers,
                },
                fragment: Some(wgpu::FragmentState {
                    module: &gpu.shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(replace_target(wgpu::TextureFormat::Rgba8UnormSrgb))],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });
    }
}

fn blend(target: wgpu::ColorTargetState) -> wgpu::BlendState {
    target.blend.expect("every board target blends")
}

#[test]
fn every_target_keeps_its_format_and_writes_every_channel() {
    let format = wgpu::TextureFormat::Bgra8UnormSrgb;
    for target in [
        replace_target(format),
        premultiplied_target(format),
        multiply_target(format),
        screen_target(format),
        alpha_target(format),
    ] {
        assert_eq!(target.format, format);
        assert_eq!(target.write_mask, wgpu::ColorWrites::ALL);
    }
}

/// Normal is a premultiplied source-over: the tile arrives with its color already scaled by
/// its alpha, so the source factor is `One` and only the destination is attenuated. `Src`
/// there instead would double-apply alpha and darken every edge.
#[test]
fn normal_blends_as_premultiplied_source_over() {
    let state = blend(premultiplied_target(wgpu::TextureFormat::Bgra8UnormSrgb));

    assert_eq!(state.color.src_factor, wgpu::BlendFactor::One);
    assert_eq!(state.color.dst_factor, wgpu::BlendFactor::OneMinusSrcAlpha);
    assert_eq!(state.color.operation, wgpu::BlendOperation::Add);
    assert_eq!(state.alpha, state.color, "alpha rides the same component");
}

/// Multiply is `src * dst` expressed as a fixed-function factor: the source is scaled *by
/// the destination* on the way in. Alpha must not be multiplied too — coverage still
/// composites normally, or a multiplied layer would eat the alpha underneath it.
#[test]
fn multiply_scales_the_source_by_the_destination_but_leaves_alpha_alone() {
    let state = blend(multiply_target(wgpu::TextureFormat::Bgra8UnormSrgb));

    assert_eq!(state.color.src_factor, wgpu::BlendFactor::Dst);
    assert_eq!(state.color.dst_factor, wgpu::BlendFactor::OneMinusSrcAlpha);
    assert_eq!(state.alpha, PREMULTIPLIED_ALPHA_COMPONENT);
}

/// Screen is `1 - (1 - src)(1 - dst)`, which factors to `src + dst * (1 - src)` — hence a
/// destination factor keyed on the source *color*, not its alpha. This is the one place the
/// two differ, and swapping them silently turns Screen back into Normal.
#[test]
fn screen_attenuates_the_destination_by_the_source_color() {
    let state = blend(screen_target(wgpu::TextureFormat::Bgra8UnormSrgb));

    assert_eq!(state.color.src_factor, wgpu::BlendFactor::One);
    assert_eq!(state.color.dst_factor, wgpu::BlendFactor::OneMinusSrc);
    assert_ne!(state.color.dst_factor, state.alpha.dst_factor);
    assert_eq!(state.alpha, PREMULTIPLIED_ALPHA_COMPONENT);
}

#[test]
fn the_three_blend_modes_compile_to_three_different_states() {
    let format = wgpu::TextureFormat::Bgra8UnormSrgb;
    let normal = blend(premultiplied_target(format));
    let multiply = blend(multiply_target(format));
    let screen = blend(screen_target(format));

    assert_ne!(normal.color, multiply.color);
    assert_ne!(normal.color, screen.color);
    assert_ne!(multiply.color, screen.color);
    assert_eq!(blend(replace_target(format)), wgpu::BlendState::REPLACE);
    assert_eq!(
        blend(alpha_target(format)),
        wgpu::BlendState::ALPHA_BLENDING
    );
}

/// `min_binding_size` is how a uniform that has grown in Rust but not in WGSL (or the other
/// way round) is caught at pipeline creation instead of read as garbage at 120 Hz.
#[test]
fn a_uniform_entry_declares_the_size_of_the_struct_it_carries() {
    let entry = uniform_entry(2, std::mem::size_of::<TileCamera>());

    assert_eq!(entry.binding, 2);
    assert_eq!(entry.count, None);
    let wgpu::BindingType::Buffer {
        ty,
        has_dynamic_offset,
        min_binding_size,
    } = entry.ty
    else {
        panic!("a uniform entry has to be a buffer");
    };
    assert_eq!(ty, wgpu::BufferBindingType::Uniform);
    assert!(!has_dynamic_offset);
    assert_eq!(
        min_binding_size.map(|n| n.get()),
        Some(std::mem::size_of::<TileCamera>() as u64)
    );
}

/// Instance attributes are hand-written offsets into a `#[repr(C)]` struct. Nothing in the
/// type system ties the two together, so a field inserted into the struct without the table
/// being updated makes every instance read the wrong bytes — with no error anywhere.
#[test]
fn every_instance_attribute_table_matches_the_struct_it_reads() {
    let cases: [(&[wgpu::VertexAttribute], usize, &str); 4] = [
        (
            TILE_INSTANCE_ATTRS,
            std::mem::size_of::<TileInstance>(),
            "tile",
        ),
        (
            STROKE_ATTRS,
            std::mem::size_of::<StrokeInstance>(),
            "stroke",
        ),
        (GUIDE_ATTRS, std::mem::size_of::<GuideInstance>(), "guide"),
        (
            VECTOR_SHAPE_ATTRS,
            std::mem::size_of::<VectorShapeInstance>(),
            "vector shape",
        ),
    ];

    for (attrs, stride, name) in cases {
        let mut cursor = 0u64;
        for (i, attr) in attrs.iter().enumerate() {
            assert_eq!(
                attr.offset, cursor,
                "{name} attribute {i} is not packed against the one before it"
            );
            assert_eq!(attr.shader_location, i as u32, "{name} location {i}");
            cursor += attr.format.size();
        }
        assert!(
            cursor <= stride as u64,
            "{name} attributes read {cursor} bytes past a {stride}-byte instance"
        );
    }
}

/// Apple Silicon reports `IntegratedGpu`, so the mapping has to keep it distinguishable
/// from a software adapter — `DeviceTier::classify` separates the two by limits, and it can
/// only do that if this does not flatten them into the same kind first.
#[test]
fn every_adapter_kind_maps_to_the_one_the_tier_table_reads() {
    assert_eq!(gpu_kind(wgpu::DeviceType::DiscreteGpu), GpuKind::Discrete);
    assert_eq!(
        gpu_kind(wgpu::DeviceType::IntegratedGpu),
        GpuKind::Integrated
    );
    assert_eq!(gpu_kind(wgpu::DeviceType::VirtualGpu), GpuKind::Integrated);
    assert_eq!(gpu_kind(wgpu::DeviceType::Cpu), GpuKind::Software);
    assert_eq!(gpu_kind(wgpu::DeviceType::Other), GpuKind::Other);
}
