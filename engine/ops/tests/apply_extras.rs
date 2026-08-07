use calumma_ops::{
    apply_output, Backend, Op, OpError, OpInput, OpKind, OpOutput, OpParams, OpRegistry,
};

struct EchoRaster;

impl Op for EchoRaster {
    fn kind(&self) -> OpKind {
        OpKind::GenerateTexture
    }

    fn backend(&self) -> Backend {
        Backend::Core
    }

    fn available(&self) -> bool {
        true
    }

    fn run(&self, input: OpInput, _params: &OpParams) -> Result<OpOutput, OpError> {
        match input {
            OpInput::Raster { rgba, w, h } => Ok(OpOutput::Raster { rgba, w, h }),
            _ => Err(OpError::BadInput),
        }
    }
}

#[test]
fn raster_output_appends_layer() {
    let mut doc = calumma_core::Document::new("id".into(), "n", 32, 32);
    let before = doc.layers.len();
    let rgba = vec![10u8, 20, 30, 255];
    apply_output(
        &mut doc,
        0,
        OpOutput::Raster {
            rgba: {
                let mut full = vec![0u8; 32 * 32 * 4];
                full[..4].copy_from_slice(&rgba);
                full
            },
            w: 32,
            h: 32,
        },
    )
    .unwrap();
    assert_eq!(doc.layers.len(), before + 1);
    assert_eq!(doc.active_layer, before);
    assert!(doc.layers[before]
        .name
        .starts_with(calumma_core::names::OP_LAYER_PREFIX));
}

#[test]
fn registry_runs_echo_raster() {
    let mut registry = OpRegistry::new();
    registry.register_core(Box::new(EchoRaster));
    let out = registry
        .run(
            OpKind::GenerateTexture,
            OpInput::Raster {
                rgba: vec![1, 2, 3, 4],
                w: 1,
                h: 1,
            },
            &OpParams::default(),
        )
        .unwrap();
    assert_eq!(
        out,
        OpOutput::Raster {
            rgba: vec![1, 2, 3, 4],
            w: 1,
            h: 1
        }
    );
}

#[test]
fn bad_mask_size_rejected() {
    let mut doc = calumma_core::Document::new("id".into(), "n", 8, 8);
    let layer = doc.active_layer;
    let err = apply_output(&mut doc, layer, OpOutput::Mask(vec![1, 2, 3])).unwrap_err();
    assert_eq!(err, OpError::BadInput);
    assert!(doc.layers[layer].mask().is_none());
}
