pub const PAPER: &str = "Paper";
pub const LAYER_ONE: &str = "Layer 1";
pub const LAYER_PREFIX: &str = "Layer";
pub const UNTITLED: &str = "Untitled";
pub const OP_LAYER_PREFIX: &str = "Op";
pub const VECTOR_LAYER_PREFIX: &str = "Vector";
pub const PASTED_LAYER_PREFIX: &str = "Pasted";

pub const ERR_OP_UNAVAILABLE: &str = "op unavailable";
pub const ERR_OP_FAILED_PREFIX: &str = "op failed: ";
pub const ERR_BAD_INPUT: &str = "bad op input";
pub const ERR_BAD_LAYER: &str = "bad layer";
pub const ERR_PLATFORM_OP_FAILED: &str = "platform op failed";
pub const ERR_EMPTY_MASK: &str = "empty mask";
pub const ERR_EMPTY_RASTER: &str = "empty raster";
pub const ERR_NO_OUTPUT: &str = "no output";
pub const ERR_MOCK_FAILURE: &str = "mock failure";

pub fn numbered_layer(n: usize) -> String {
    format!("{LAYER_PREFIX} {n}")
}

pub fn numbered_op_layer(n: usize) -> String {
    format!("{OP_LAYER_PREFIX} {n}")
}

pub fn numbered_vector_layer(n: usize) -> String {
    format!("{VECTOR_LAYER_PREFIX} {n}")
}

pub fn numbered_pasted_layer(n: usize) -> String {
    format!("{PASTED_LAYER_PREFIX} {n}")
}

pub fn duplicate_layer_name(base: &str) -> String {
    format!("{base} copy")
}
