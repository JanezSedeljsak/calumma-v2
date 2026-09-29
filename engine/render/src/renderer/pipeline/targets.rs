//! Color target states: the fixed-function blends Normal, Multiply and Screen compile to, and the
//! plain replace and alpha targets the rest of the board draws with.

pub(in crate::renderer) fn replace_target(format: wgpu::TextureFormat) -> wgpu::ColorTargetState {
    wgpu::ColorTargetState {
        format,
        blend: Some(wgpu::BlendState::REPLACE),
        write_mask: wgpu::ColorWrites::ALL,
    }
}

pub(crate) const PREMULTIPLIED_ALPHA_COMPONENT: wgpu::BlendComponent = wgpu::BlendComponent {
    src_factor: wgpu::BlendFactor::One,
    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
    operation: wgpu::BlendOperation::Add,
};

pub(crate) fn premultiplied_target(format: wgpu::TextureFormat) -> wgpu::ColorTargetState {
    wgpu::ColorTargetState {
        format,
        blend: Some(wgpu::BlendState {
            color: PREMULTIPLIED_ALPHA_COMPONENT,
            alpha: PREMULTIPLIED_ALPHA_COMPONENT,
        }),
        write_mask: wgpu::ColorWrites::ALL,
    }
}

pub(in crate::renderer) fn multiply_target(format: wgpu::TextureFormat) -> wgpu::ColorTargetState {
    wgpu::ColorTargetState {
        format,
        blend: Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Dst,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: PREMULTIPLIED_ALPHA_COMPONENT,
        }),
        write_mask: wgpu::ColorWrites::ALL,
    }
}

pub(in crate::renderer) fn screen_target(format: wgpu::TextureFormat) -> wgpu::ColorTargetState {
    wgpu::ColorTargetState {
        format,
        blend: Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrc,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: PREMULTIPLIED_ALPHA_COMPONENT,
        }),
        write_mask: wgpu::ColorWrites::ALL,
    }
}

pub(in crate::renderer) fn alpha_target(format: wgpu::TextureFormat) -> wgpu::ColorTargetState {
    wgpu::ColorTargetState {
        format,
        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
        write_mask: wgpu::ColorWrites::ALL,
    }
}
