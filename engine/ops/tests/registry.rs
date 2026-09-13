use calumma_core::Document;
use calumma_ops::{
    apply_output, run_op, run_op_on_document, OpError, OpInput, OpKind, OpOutput, OpParams,
    OpRegistry,
};

mod mocks {
    use calumma_ops::{Op, OpError, OpInput, OpKind, OpOutput, OpParams};

    pub struct MockOp {
        pub kind: OpKind,
        pub available: bool,
        pub fail: bool,
        pub output: OpOutput,
    }

    impl MockOp {
        pub fn unavailable(kind: OpKind) -> Self {
            Self {
                kind,
                available: false,
                fail: false,
                output: OpOutput::Mask(Vec::new()),
            }
        }

        pub fn failing(kind: OpKind) -> Self {
            Self {
                kind,
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

        fn available(&self) -> bool {
            self.available
        }

        fn run(&self, _input: OpInput, _params: &OpParams) -> Result<OpOutput, OpError> {
            if self.fail {
                Err(OpError::Failed("mock failure".into()))
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
fn unavailable_everywhere_is_gated() {
    let mut registry = OpRegistry::new();
    registry.register(Box::new(MockOp::unavailable(OpKind::SmartMatte)));
    registry.register(Box::new(MockOp::unavailable(OpKind::SmartMatte)));

    assert!(!registry.available(OpKind::SmartMatte));
    let err = run_op(
        &registry,
        OpKind::SmartMatte,
        OpInput {
            rgba: Vec::new(),
            w: 0,
            h: 0,
        },
        &OpParams::default(),
    )
    .unwrap_err();
    assert_eq!(err, OpError::Unavailable);
}

#[test]
fn error_propagates_through_registry() {
    let mut registry = OpRegistry::new();
    registry.register(Box::new(MockOp::failing(OpKind::SmartMatte)));
    let err = run_op(
        &registry,
        OpKind::SmartMatte,
        OpInput {
            rgba: Vec::new(),
            w: 0,
            h: 0,
        },
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
    registry.register(Box::new(MockOp::failing(OpKind::SmartMatte)));

    let err = run_op_on_document(
        &registry,
        &mut doc,
        layer,
        OpKind::SmartMatte,
        &OpParams::default(),
    )
    .unwrap_err();
    assert!(matches!(err, OpError::Failed(_)));
    assert_eq!(doc.layers.len(), layers);
    assert_eq!(doc.layers[layer].tiles().unwrap().get_pixel(8, 8), pixel);
    assert_eq!(doc.history.can_undo(), could_undo);
}

#[test]
fn a_mask_output_bakes_into_pixels() {
    let mut doc = Document::new("p".into(), "P", 32, 32);
    let layer = doc.active_layer;
    doc.layers[layer]
        .tiles_mut()
        .unwrap()
        .set_pixel(4, 4, [1, 2, 3, 255]);
    let pixel = doc.layers[layer].tiles().unwrap().get_pixel(4, 4);

    apply_output(&mut doc, layer, mask_output(32, 32)).unwrap();

    assert_eq!(doc.layers[layer].tiles().unwrap().get_pixel(4, 4), pixel);
    assert!(doc.history.can_undo());
}
