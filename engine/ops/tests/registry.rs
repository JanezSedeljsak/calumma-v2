use calumma_core::{Document, LayerContent};
use calumma_ops::{
    apply_output, run_op, run_op_on_document, Backend, OpError, OpInput, OpKind, OpOutput,
    OpParams, OpRegistry,
};

mod mocks {
    use calumma_ops::{Backend, Op, OpError, OpInput, OpKind, OpOutput, OpParams};

    pub struct MockOp {
        pub kind: OpKind,
        pub backend: Backend,
        pub available: bool,
        pub fail: bool,
        pub output: OpOutput,
    }

    impl MockOp {
        pub fn ok(kind: OpKind, backend: Backend, output: OpOutput) -> Self {
            Self {
                kind,
                backend,
                available: true,
                fail: false,
                output,
            }
        }

        pub fn unavailable(kind: OpKind, backend: Backend) -> Self {
            Self {
                kind,
                backend,
                available: false,
                fail: false,
                output: OpOutput::Mask(Vec::new()),
            }
        }

        pub fn failing(kind: OpKind, backend: Backend) -> Self {
            Self {
                kind,
                backend,
                available: true,
                fail: true,
                output: OpOutput::Mask(Vec::new()),
            }
        }
    }

    impl Op for MockOp {
        fn kind(&self) -> OpKind {
            self.kind
        }

        fn backend(&self) -> Backend {
            self.backend
        }

        fn available(&self) -> bool {
            self.available
        }

        fn run(&self, _input: OpInput, _params: &OpParams) -> Result<OpOutput, OpError> {
            if self.fail {
                Err(OpError::Failed(
                    calumma_core::names::ERR_MOCK_FAILURE.into(),
                ))
            } else {
                Ok(self.output.clone())
            }
        }
    }
}

use mocks::MockOp;

fn mask_output(w: u32, h: u32) -> OpOutput {
    OpOutput::Mask(vec![255u8; (w * h) as usize])
}

#[test]
fn platform_beats_core_when_available() {
    let mut registry = OpRegistry::new();
    registry.register_core(Box::new(MockOp::ok(
        OpKind::RemoveBackground,
        Backend::Core,
        OpOutput::Mask(vec![1]),
    )));
    registry.register_platform(Box::new(MockOp::ok(
        OpKind::RemoveBackground,
        Backend::Platform,
        OpOutput::Mask(vec![9]),
    )));

    assert_eq!(
        registry.backend_for(OpKind::RemoveBackground),
        Some(Backend::Platform)
    );
    let out = run_op(
        &registry,
        OpKind::RemoveBackground,
        OpInput::None,
        &OpParams::default(),
    )
    .unwrap();
    assert_eq!(out, OpOutput::Mask(vec![9]));
}

#[test]
fn unavailable_platform_falls_back_to_core() {
    let mut registry = OpRegistry::new();
    registry.register_core(Box::new(MockOp::ok(
        OpKind::SuggestShape,
        Backend::Core,
        OpOutput::Paths(Vec::new()),
    )));
    registry.register_platform(Box::new(MockOp::unavailable(
        OpKind::SuggestShape,
        Backend::Platform,
    )));

    assert_eq!(
        registry.backend_for(OpKind::SuggestShape),
        Some(Backend::Core)
    );
}

#[test]
fn unavailable_everywhere_is_gated() {
    let mut registry = OpRegistry::new();
    registry.register_core(Box::new(MockOp::unavailable(
        OpKind::GenerateTexture,
        Backend::Core,
    )));
    registry.register_platform(Box::new(MockOp::unavailable(
        OpKind::GenerateTexture,
        Backend::Platform,
    )));

    assert!(!registry.available(OpKind::GenerateTexture));
    let err = run_op(
        &registry,
        OpKind::GenerateTexture,
        OpInput::None,
        &OpParams::default(),
    )
    .unwrap_err();
    assert_eq!(err, OpError::Unavailable);
}

#[test]
fn error_propagates_through_registry() {
    let mut registry = OpRegistry::new();
    registry.register_core(Box::new(MockOp::failing(OpKind::Vectorize, Backend::Core)));
    let err = run_op(
        &registry,
        OpKind::Vectorize,
        OpInput::None,
        &OpParams::default(),
    )
    .unwrap_err();
    assert!(matches!(err, OpError::Failed(_)));
}

#[test]
fn failed_op_leaves_document_and_history_untouched() {
    let mut doc = Document::new("p".into(), "P", 64, 64);
    let layer = doc.active_layer;
    doc.layers[layer]
        .tiles_mut()
        .unwrap()
        .set_pixel(8, 8, [10, 20, 30, 255]);
    let pixel = doc.layers[layer].tiles().unwrap().get_pixel(8, 8);
    let layers = doc.layers.len();
    let could_undo = doc.history.can_undo();

    let mut registry = OpRegistry::new();
    registry.register_core(Box::new(MockOp::failing(
        OpKind::RemoveBackground,
        Backend::Core,
    )));

    let err = run_op_on_document(
        &registry,
        &mut doc,
        layer,
        OpKind::RemoveBackground,
        &OpParams::default(),
    )
    .unwrap_err();
    assert!(matches!(err, OpError::Failed(_)));
    assert_eq!(doc.layers.len(), layers);
    assert_eq!(doc.layers[layer].tiles().unwrap().get_pixel(8, 8), pixel);
    assert!(doc.layers[layer].mask().is_none());
    assert_eq!(doc.history.can_undo(), could_undo);
}

#[test]
fn mask_attach_does_not_mutate_pixels() {
    let mut doc = Document::new("p".into(), "P", 32, 32);
    let layer = doc.active_layer;
    doc.layers[layer]
        .tiles_mut()
        .unwrap()
        .set_pixel(4, 4, [1, 2, 3, 255]);
    let pixel = doc.layers[layer].tiles().unwrap().get_pixel(4, 4);

    apply_output(&mut doc, layer, mask_output(32, 32)).unwrap();

    assert_eq!(doc.layers[layer].tiles().unwrap().get_pixel(4, 4), pixel);
    assert_eq!(doc.layers[layer].mask().map(|m| m.len()), Some(32 * 32));
}

#[test]
fn paths_output_adds_vector_layer() {
    let mut doc = Document::new("p".into(), "P", 32, 32);
    let before = doc.layers.len();
    let paths = vec![calumma_core::VectorPath {
        points: vec![(1.0, 2.0), (3.0, 4.0)],
        closed: false,
        fill: false,
        color: [0, 0, 0, 255],
        stroke_width: 1.0,
    }];
    apply_output(&mut doc, 0, OpOutput::Paths(paths)).unwrap();
    assert_eq!(doc.layers.len(), before + 1);
    assert!(matches!(
        doc.layers.last().unwrap().content,
        LayerContent::Vector(_)
    ));
}

#[test]
fn mock_per_kind_registers() {
    let mut registry = OpRegistry::new();
    for kind in [
        OpKind::RemoveBackground,
        OpKind::GenerateTexture,
        OpKind::Vectorize,
        OpKind::SuggestShape,
    ] {
        registry.register_core(Box::new(MockOp::ok(
            kind,
            Backend::Core,
            OpOutput::Mask(vec![0]),
        )));
        assert!(registry.available(kind));
    }
}
