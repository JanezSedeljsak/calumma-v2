//! The render pipelines `Renderer::assemble` builds, grouped by the bind layout they share.

use super::*;

pub(in crate::renderer) struct TilePipelines {
    pub(super) normal: wgpu::RenderPipeline,
    pub(super) multiply: wgpu::RenderPipeline,
    pub(super) screen: wgpu::RenderPipeline,
    pub(super) solid_normal: wgpu::RenderPipeline,
    pub(super) solid_multiply: wgpu::RenderPipeline,
    pub(super) solid_screen: wgpu::RenderPipeline,
    pub(super) tile_blend: wgpu::RenderPipeline,
    pub(super) solid_blend: wgpu::RenderPipeline,
}

pub(in crate::renderer) struct PreviewPipelines {
    pub(super) stroke: wgpu::RenderPipeline,
    pub(super) overlay: wgpu::RenderPipeline,
    pub(super) guide: wgpu::RenderPipeline,
    pub(super) shape: wgpu::RenderPipeline,
    pub(super) vector_shape: wgpu::RenderPipeline,
}

pub(in crate::renderer) fn paper_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    paper_bgl: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let paper_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("paper-pl"),
        bind_group_layouts: &[Some(paper_bgl)],
        ..Default::default()
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("paper"),
        layout: Some(&paper_pl),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_paper"),
            compilation_options: Default::default(),
            targets: &[Some(replace_target(format))],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

/// Every pipeline that draws layer content: tiles and solid tiles under the three fixed-function
/// blend states, and the two backdrop-reading pipelines every other blend mode goes through.
pub(in crate::renderer) fn tile_pipelines(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    tile_shared_bgl: &wgpu::BindGroupLayout,
    backdrop_bgl: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> TilePipelines {
    let tile_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("tile-pl"),
        bind_group_layouts: &[Some(tile_shared_bgl)],
        ..Default::default()
    });
    let tile_instance_layout = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<TileInstance>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: TILE_INSTANCE_ATTRS,
    };
    let tile_pipeline_for = |label: &str, target: wgpu::ColorTargetState| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&tile_pl),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_tile"),
                compilation_options: Default::default(),
                buffers: &[Some(tile_instance_layout.clone())],
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_tile"),
                compilation_options: Default::default(),
                targets: &[Some(target)],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        })
    };
    let tile_pipeline_normal = tile_pipeline_for("tile-normal", premultiplied_target(format));
    let tile_pipeline_multiply = tile_pipeline_for("tile-multiply", multiply_target(format));
    let tile_pipeline_screen = tile_pipeline_for("tile-screen", screen_target(format));

    let solid_pipeline_for = |label: &str, target: wgpu::ColorTargetState| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&tile_pl),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_doc_quad"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_solid_tile"),
                compilation_options: Default::default(),
                targets: &[Some(target)],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        })
    };
    let solid_pipeline_normal = solid_pipeline_for("solid-normal", premultiplied_target(format));
    let solid_pipeline_multiply = solid_pipeline_for("solid-multiply", multiply_target(format));
    let solid_pipeline_screen = solid_pipeline_for("solid-screen", screen_target(format));

    let blend_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("tile-blend-pl"),
        bind_group_layouts: &[Some(tile_shared_bgl), Some(backdrop_bgl)],
        ..Default::default()
    });
    let blend_pipeline_for =
        |label: &str, vs: &str, fs: &str, buffers: &[Option<wgpu::VertexBufferLayout>]| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&blend_pl),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers,
                },
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(replace_target(format))],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
    let tile_blend_pipeline = blend_pipeline_for(
        "tile-blend",
        "vs_tile",
        "fs_tile_blend",
        &[Some(tile_instance_layout.clone())],
    );
    let solid_blend_pipeline =
        blend_pipeline_for("solid-blend", "vs_doc_quad", "fs_solid_tile_blend", &[]);
    TilePipelines {
        normal: tile_pipeline_normal,
        multiply: tile_pipeline_multiply,
        screen: tile_pipeline_screen,
        solid_normal: solid_pipeline_normal,
        solid_multiply: solid_pipeline_multiply,
        solid_screen: solid_pipeline_screen,
        tile_blend: tile_blend_pipeline,
        solid_blend: solid_blend_pipeline,
    }
}

/// The pipelines that share the preview uniform: ink strokes, screen-space chrome, guides, the
/// shape preview and vector shapes.
pub(in crate::renderer) fn preview_pipelines(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    preview_bgl: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> PreviewPipelines {
    let preview_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("preview-pl"),
        bind_group_layouts: &[Some(preview_bgl)],
        ..Default::default()
    });

    let stroke_layout = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<StrokeInstance>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: STROKE_ATTRS,
    };

    let stroke_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("stroke"),
        layout: Some(&preview_pl),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_stroke"),
            compilation_options: Default::default(),
            buffers: &[Some(stroke_layout)],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_stroke"),
            compilation_options: Default::default(),
            targets: &[Some(alpha_target(format))],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    let overlay_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("overlay"),
        layout: Some(&preview_pl),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_overlay"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<StrokeInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: STROKE_ATTRS,
            })],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_overlay"),
            compilation_options: Default::default(),
            targets: &[Some(alpha_target(format))],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    let guide_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("guide"),
        layout: Some(&preview_pl),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_guide"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<GuideInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: GUIDE_ATTRS,
            })],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_guide"),
            compilation_options: Default::default(),
            targets: &[Some(alpha_target(format))],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    let shape_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("shape-preview"),
        layout: Some(&preview_pl),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_shape_preview"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_shape_preview"),
            compilation_options: Default::default(),
            targets: &[Some(alpha_target(format))],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    let vector_shape_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("vector-shape"),
        layout: Some(&preview_pl),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_vector_shape"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<VectorShapeInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: VECTOR_SHAPE_ATTRS,
            })],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_vector_shape"),
            compilation_options: Default::default(),
            targets: &[Some(alpha_target(format))],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    PreviewPipelines {
        stroke: stroke_pipeline,
        overlay: overlay_pipeline,
        guide: guide_pipeline,
        shape: shape_pipeline,
        vector_shape: vector_shape_pipeline,
    }
}

pub(in crate::renderer) fn vector_fill_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    fill_bgl: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let fill_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("vector-fill-pl"),
        bind_group_layouts: &[Some(fill_bgl)],
        ..Default::default()
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("vector-fill"),
        layout: Some(&fill_pl),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_vector_fill"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<crate::vector_draw::VectorFillInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: VECTOR_FILL_ATTRS,
            })],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_vector_fill"),
            compilation_options: Default::default(),
            targets: &[Some(alpha_target(format))],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}
