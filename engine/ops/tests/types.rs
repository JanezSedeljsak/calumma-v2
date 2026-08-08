use calumma_core::names::{ERR_BAD_INPUT, ERR_BAD_LAYER, ERR_OP_FAILED_PREFIX, ERR_OP_UNAVAILABLE};
use calumma_ops::OpError;
use std::error::Error;

#[test]
fn op_error_display_uses_the_shared_name_constants() {
    assert_eq!(OpError::Unavailable.to_string(), ERR_OP_UNAVAILABLE);
    assert_eq!(OpError::BadInput.to_string(), ERR_BAD_INPUT);
    assert_eq!(OpError::BadLayer.to_string(), ERR_BAD_LAYER);
    assert_eq!(
        OpError::Failed("vision said no".into()).to_string(),
        format!("{ERR_OP_FAILED_PREFIX}vision said no")
    );
}

#[test]
fn op_error_is_a_std_error() {
    let err: Box<dyn Error> = Box::new(OpError::BadLayer);
    assert_eq!(err.to_string(), ERR_BAD_LAYER);
    assert!(err.source().is_none());
}

#[test]
fn op_error_variants_compare_by_value() {
    assert_eq!(OpError::BadInput, OpError::BadInput);
    assert_ne!(OpError::BadInput, OpError::BadLayer);
    assert_eq!(
        OpError::Failed("x".into()),
        OpError::Failed(String::from("x"))
    );
    assert_ne!(OpError::Failed("x".into()), OpError::Failed("y".into()));
}
