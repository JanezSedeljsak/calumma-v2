//! The rule table, asserted directly, plus the guarantee that the engine's own early returns
//! agree with it. A rule that only the panel knows would let a greyed-out button and a silent
//! refusal drift apart, which is the whole reason `tool_block` exists.

use calumma_core::vector::{VectorItem, VectorShape};
use calumma_core::*;

const DOC: u32 = 128;

const PAINT_TOOLS: [Tool; 4] = [Tool::Pen, Tool::Eraser, Tool::Blur, Tool::Fill];
const SELECT_TOOLS: [Tool; 4] = [
    Tool::SelectRect,
    Tool::SelectEllipse,
    Tool::SelectLasso,
    Tool::MagicWand,
];
const SHAPE_TOOLS: [Tool; 6] = [
    Tool::Line,
    Tool::Rect,
    Tool::Ellipse,
    Tool::Arrow,
    Tool::Triangle,
    Tool::Pentagon,
];

fn board() -> Document {
    let mut doc = Document::new("p".into(), "t", DOC, DOC);
    doc.resize_viewport(DOC as f32, DOC as f32, 1.0);
    doc.camera.zoom = 1.0;
    doc.camera.pan_x = 0.0;
    doc.camera.pan_y = 0.0;
    doc.add_layer("Paint");
    doc
}

fn rect_item() -> VectorItem {
    VectorItem::Shape(VectorShape {
        shape: Shape {
            tool: Tool::Rect,
            start: (10.0, 10.0),
            end: (60.0, 60.0),
            half_width: 1.0,
            fill: true,
            stroke: false,
        },
        color: [255, 0, 0, 255],
        stroke_color: [255, 0, 0, 255],
    })
}

fn with_text() -> Document {
    let mut doc = board();
    doc.tool = Tool::Text;
    doc.pointer_down(40.0, 40.0);
    doc.text_insert("hi");
    doc.commit_text();
    doc.set_active_layer(doc.layers.len() - 1);
    doc
}

fn with_vector() -> Document {
    let mut doc = board();
    doc.add_vector_layer("V", rect_item());
    doc.set_active_layer(doc.layers.len() - 1);
    doc
}

fn painted() -> Document {
    let mut doc = board();
    doc.tool = Tool::Pen;
    doc.pointer_down(40.0, 40.0);
    doc.pointer_up(60.0, 60.0);
    doc
}

#[test]
fn a_raster_layer_blocks_nothing() {
    let mut doc = painted();
    for tool in PAINT_TOOLS
        .into_iter()
        .chain(SELECT_TOOLS)
        .chain(SHAPE_TOOLS)
        .chain([Tool::Move, Tool::Text, Tool::Eyedropper, Tool::Transform])
    {
        assert_eq!(doc.tool_block(tool), ToolBlock::None, "{tool:?}");
    }
    doc.vector_mode = true;
    for tool in PAINT_TOOLS.into_iter().chain(SHAPE_TOOLS) {
        assert_eq!(
            doc.tool_block(tool),
            ToolBlock::None,
            "{tool:?} in vector mode"
        );
    }
}

/// Move, transform and the text tool, and nothing else — what the layer is *for* is the one
/// thing you can still do to it.
#[test]
fn a_text_layer_leaves_move_transform_and_text() {
    let doc = with_text();
    for tool in [Tool::Move, Tool::Transform, Tool::Text, Tool::Eyedropper] {
        assert_eq!(doc.tool_block(tool), ToolBlock::None, "{tool:?}");
    }
    for tool in PAINT_TOOLS
        .into_iter()
        .chain(SELECT_TOOLS)
        .chain(SHAPE_TOOLS)
    {
        assert_eq!(doc.tool_block(tool), ToolBlock::TextLayer, "{tool:?}");
    }
}

/// A vector layer keeps the tools that can produce another vector — the pen and the shapes,
/// with vector mode pinned on under them — and refuses the ones that need pixels.
#[test]
fn a_vector_layer_leaves_the_tools_that_draw_vectors() {
    let doc = with_vector();
    assert!(doc.vector_mode_locked());
    assert!(
        doc.effective_vector_mode(),
        "without the shell knob being on"
    );

    for tool in SHAPE_TOOLS.into_iter().chain([
        Tool::Pen,
        Tool::Move,
        Tool::Transform,
        Tool::Text,
        Tool::Eyedropper,
    ]) {
        assert_eq!(doc.tool_block(tool), ToolBlock::None, "{tool:?}");
    }
    for tool in [Tool::Eraser, Tool::Blur, Tool::Fill]
        .into_iter()
        .chain(SELECT_TOOLS)
    {
        assert_eq!(doc.tool_block(tool), ToolBlock::VectorLayer, "{tool:?}");
    }
}

#[test]
fn a_locked_layer_refuses_everything_that_would_change_it() {
    let mut doc = painted();
    doc.set_layer_locked(doc.active_layer, true);
    for tool in PAINT_TOOLS
        .into_iter()
        .chain(SELECT_TOOLS)
        .chain(SHAPE_TOOLS)
        .chain([Tool::Text, Tool::Transform])
    {
        assert_eq!(doc.tool_block(tool), ToolBlock::LayerLocked, "{tool:?}");
    }
    assert_eq!(
        doc.tool_block(Tool::Eyedropper),
        ToolBlock::None,
        "reading the composite changes nothing"
    );
    assert_eq!(
        doc.tool_block(Tool::Move),
        ToolBlock::None,
        "move picks its own target out of the stack"
    );

    doc.vector_mode = true;
    assert_eq!(
        doc.tool_block(Tool::Pen),
        ToolBlock::None,
        "a vector pen commits into a layer that does not exist yet"
    );
}

#[test]
fn an_empty_layer_has_nothing_to_transform() {
    let doc = board();
    assert_eq!(doc.tool_block(Tool::Transform), ToolBlock::NoContent);
    assert_eq!(doc.tool_block(Tool::Pen), ToolBlock::None);
}

/// The regression the whole plan is about: every guard the engine already had now agrees with
/// the table, so a tool the panel greys out is a tool the engine refuses, and the reverse.
#[test]
fn the_engines_own_guards_agree_with_the_table() {
    for mut doc in [with_text(), with_vector()] {
        let index = doc.active_layer;
        let before = doc.layers[index].clone();

        doc.tool = Tool::Eraser;
        doc.pointer_down(20.0, 20.0);
        doc.pointer_up(70.0, 70.0);

        doc.tool = Tool::Blur;
        doc.pointer_down(20.0, 20.0);
        doc.pointer_up(70.0, 70.0);

        doc.tool = Tool::Fill;
        doc.pointer_down(30.0, 30.0);

        doc.tool = Tool::SelectRect;
        doc.pointer_down(20.0, 20.0);
        doc.pointer_up(70.0, 70.0);
        assert!(doc.selection.is_none(), "no marquee you could not then use");

        doc.tool = Tool::SelectLasso;
        doc.pointer_down(20.0, 20.0);
        doc.pointer_move(40.0, 20.0);
        doc.pointer_move(40.0, 40.0);
        doc.pointer_up(20.0, 40.0);
        assert!(doc.selection.is_none());

        doc.tool = Tool::MagicWand;
        doc.pointer_down(30.0, 30.0);
        assert!(doc.selection.is_none());

        assert_eq!(
            doc.layers[index], before,
            "the layer came through untouched"
        );
    }
}

/// A refusal speaks once per (layer, tool) pair. It has to speak again after the user moves on
/// and comes back, or the second surprise would land in silence.
#[test]
fn a_refusal_explains_itself_once() {
    let mut doc = with_text();
    doc.tool = Tool::Pen;

    doc.pointer_down(20.0, 20.0);
    assert_eq!(doc.take_tool_block_notice(), Some(ToolBlock::TextLayer));

    doc.pointer_down(25.0, 25.0);
    assert_eq!(
        doc.take_tool_block_notice(),
        None,
        "said once, not per press"
    );

    doc.tool = Tool::Eraser;
    doc.pointer_down(25.0, 25.0);
    assert_eq!(
        doc.take_tool_block_notice(),
        Some(ToolBlock::TextLayer),
        "a different tool is a different question"
    );
}

/// Selection survives a trip through a layer that cannot make one — only *creating* a marquee
/// is blocked, never keeping one, so select-here-paste-there still works.
#[test]
fn a_marquee_survives_a_visit_to_a_text_layer() {
    let mut doc = painted();
    doc.tool = Tool::SelectRect;
    doc.pointer_down(10.0, 10.0);
    doc.pointer_up(50.0, 50.0);
    assert!(doc.selection.is_some());

    let raster = doc.active_layer;
    doc.tool = Tool::Text;
    doc.pointer_down(80.0, 80.0);
    doc.text_insert("x");
    doc.commit_text();
    doc.set_active_layer(doc.layers.len() - 1);
    assert!(doc.selection.is_some(), "the marquee is document state");

    doc.set_active_layer(raster);
    assert!(doc.selection.is_some());
}

#[test]
fn rasterizing_hands_the_blocked_tools_back() {
    for mut doc in [with_text(), with_vector()] {
        let index = doc.active_layer;
        assert!(doc.layer_is_rasterizable(index));
        assert!(doc.tool_blocked(Tool::Eraser));

        assert!(doc.rasterize_layer(index));

        assert!(!doc.layer_is_rasterizable(index));
        assert!(!doc.vector_mode_locked());
        for tool in PAINT_TOOLS
            .into_iter()
            .chain(SELECT_TOOLS)
            .chain(SHAPE_TOOLS)
        {
            assert_eq!(doc.tool_block(tool), ToolBlock::None, "{tool:?}");
        }
        assert!(
            doc.layers[index].tiles().is_some_and(|t| !t.is_empty()),
            "what was on screen is still on screen, as pixels"
        );
    }
}

#[test]
fn rasterizing_a_layer_that_is_already_pixels_does_nothing() {
    let mut doc = painted();
    let index = doc.active_layer;
    assert!(!doc.layer_is_rasterizable(index));
    assert!(!doc.rasterize_layer(index));
}
