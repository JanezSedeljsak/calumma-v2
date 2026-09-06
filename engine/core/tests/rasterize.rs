//! `rasterize_layer`'s guard clauses — the happy paths (a real text or vector layer flattening
//! into pixels) are already exercised end to end in `tool_gate.rs`'s
//! `rasterizing_hands_the_blocked_tools_back`; this covers the refusals that dispatch never
//! reaches by itself.

use calumma_core::shape::{Shape, Tool};
use calumma_core::vector::{VectorItem, VectorShape};
use calumma_core::*;

fn filled_rect() -> VectorItem {
    VectorItem::Shape(VectorShape {
        shape: Shape {
            tool: Tool::Rect,
            start: (2.0, 2.0),
            end: (10.0, 10.0),
            half_width: 1.0,
            fill: true,
            stroke: false,
        },
        color: [255, 0, 0, 255],
        stroke_color: [255, 0, 0, 255],
    })
}

#[test]
fn rasterizing_a_vector_layer_in_a_zero_sized_document_does_nothing() {
    let mut doc = Document::new("p".into(), "t", 32, 32);
    let index = doc.add_vector_layer("V", filled_rect());
    doc.width = 0;
    assert!(!doc.rasterize_vector_layer(index));
    assert!(
        doc.layer_is_rasterizable(index),
        "the layer itself is untouched"
    );
}

#[test]
fn rasterizing_an_out_of_range_index_does_nothing() {
    let mut doc = Document::new("p".into(), "t", 32, 32);
    let past_the_end = doc.layers.len();
    assert!(!doc.rasterize_vector_layer(past_the_end));
    assert!(!doc.rasterize_layer(past_the_end));
    assert!(!doc.layer_is_rasterizable(past_the_end));
}

#[test]
fn rasterizing_a_layer_that_has_no_vector_item_does_nothing() {
    let mut doc = Document::new("p".into(), "t", 32, 32);
    let raster = doc.active_layer;
    assert!(!doc.rasterize_vector_layer(raster));
    assert!(!doc.rasterize_layer(raster));
    assert!(!doc.layer_is_rasterizable(raster));
}
