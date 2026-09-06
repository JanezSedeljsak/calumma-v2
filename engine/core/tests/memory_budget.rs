use calumma_core::limits::{HISTORY_MEMORY_BUDGET_BYTES, PAPER_WHITE};
use calumma_core::memory::{document_memory, DocumentMemory};
use calumma_core::tile::{TILE_BYTES, TILE_SIZE};
use calumma_core::{Document, History, Tool};
use std::collections::HashSet;
use std::sync::Arc;

const SIDE: u32 = TILE_SIZE * 8;

fn identity_doc() -> Document {
    let mut doc = Document::new("p".into(), "t", SIDE, SIDE);
    doc.resize_viewport(SIDE as f32, SIDE as f32, 1.0);
    doc.camera.zoom = 1.0;
    doc.camera.pan_x = 0.0;
    doc.camera.pan_y = 0.0;
    doc.tool = Tool::Pen;
    doc.brush_size = 96.0;
    doc.color = [0, 0, 0, 255];
    doc
}

fn unique_live_tile_bytes(doc: &Document) -> usize {
    let mut seen = HashSet::new();
    let mut bytes = 0usize;
    for layer in &doc.layers {
        let Some(tiles) = layer.tiles() else {
            continue;
        };
        for coord in tiles.coords() {
            let Some(tile) = tiles.get(coord) else {
                continue;
            };
            if seen.insert(Arc::as_ptr(tile) as usize) {
                bytes += tile.capacity();
            }
        }
    }
    bytes
}

fn assert_report_bounds_live_tiles(doc: &Document, report: &DocumentMemory) {
    let live = unique_live_tile_bytes(doc);
    assert_eq!(
        report.tile_bytes, live,
        "document_memory counts each live tile allocation once"
    );
    let unique_tiles = report.tile_count - report.shared_tile_count;
    assert_eq!(
        report.tile_bytes,
        unique_tiles * TILE_BYTES,
        "every unique tile is a full {TILE_BYTES}-byte allocation"
    );
    let naive = report.tile_count * TILE_BYTES
        + report.mask_bytes
        + report.vector_bytes
        + report.text_bytes
        + report.preview_bytes
        + report.history_bytes;
    assert!(
        report.total() <= naive,
        "sharing can only lower the total below a naive per-reference sum"
    );
    assert!(
        doc.history.memory_used() <= HISTORY_MEMORY_BUDGET_BYTES,
        "history.memory_used {} exceeded the {} budget",
        doc.history.memory_used(),
        HISTORY_MEMORY_BUDGET_BYTES
    );
}

fn stroke(doc: &mut Document, x0: f32, y0: f32, x1: f32, y1: f32) {
    doc.pointer_down(x0, y0);
    doc.pointer_move(x1, y1);
    doc.pointer_up(x1, y1);
}

#[test]
fn heavy_strokes_stay_inside_the_history_budget() {
    let mut doc = identity_doc();
    let tile_budget = 12 * TILE_BYTES;
    doc.history = History::new(tile_budget);
    let baseline = document_memory(&doc);

    for i in 0..64 {
        let t = i as f32;
        stroke(
            &mut doc,
            24.0 + t * 28.0,
            40.0,
            24.0 + t * 28.0,
            SIDE as f32 - 40.0,
        );
        let report = document_memory(&doc);
        assert_report_bounds_live_tiles(&doc, &report);
        if doc.history.undo_depth() > 1 {
            assert!(
                doc.history.memory_used() <= tile_budget,
                "stroke {i} pushed history to {} bytes over a {tile_budget} budget",
                doc.history.memory_used()
            );
        }
        assert!(
            report.total() >= baseline.total(),
            "painting cannot shrink the document below an empty board"
        );
    }

    assert!(doc.history.can_undo());
    assert!(
        doc.history.undo_depth() < 64,
        "a tiny budget must evict old strokes rather than grow without bound"
    );

    let painted = document_memory(&doc);
    assert!(painted.tile_bytes > baseline.tile_bytes);

    while doc.undo() {}
    let after_undo = document_memory(&doc);
    assert!(!doc.history.can_undo());
    assert_report_bounds_live_tiles(&doc, &after_undo);
    assert!(
        after_undo.tile_bytes <= painted.tile_bytes,
        "undoing the surviving stack cannot grow live tiles"
    );
}

#[test]
fn paint_undo_cycles_do_not_leak_unique_tiles() {
    let mut doc = identity_doc();
    doc.history = History::new(64 * TILE_BYTES);
    let baseline = document_memory(&doc);

    for round in 0..24 {
        stroke(
            &mut doc,
            80.0,
            80.0 + round as f32 * 40.0,
            SIDE as f32 - 80.0,
            80.0 + round as f32 * 40.0,
        );
        assert!(doc.undo(), "round {round} must be undoable");
        let report = document_memory(&doc);
        assert_eq!(
            report.tile_bytes, baseline.tile_bytes,
            "round {round}: live tiles after undo must match the empty board"
        );
        assert_report_bounds_live_tiles(&doc, &report);
    }

    stroke(&mut doc, 32.0, 32.0, 64.0, 64.0);
    let after_new_stroke = document_memory(&doc);
    assert!(
        after_new_stroke.history_bytes <= 4 * TILE_BYTES,
        "a new stroke clears redo, so the undone cycles must not keep their tiles"
    );
}

#[test]
fn forking_tiles_then_dropping_history_releases_them() {
    let mut doc = identity_doc();
    let before = document_memory(&doc).tile_bytes;
    assert_eq!(before, TILE_BYTES);

    for y in 0..8 {
        for x in 0..8 {
            doc.layers[doc.active_layer].tiles_mut().unwrap().set_pixel(
                (x * TILE_SIZE + 4) as i32,
                (y * TILE_SIZE + 4) as i32,
                [1, 2, 3, 255],
            );
        }
    }
    let painted = document_memory(&doc);
    assert_eq!(
        painted.tile_bytes,
        TILE_BYTES * 65,
        "paper plus one forked allocation per tile of the 8x8 grid"
    );
    assert_report_bounds_live_tiles(&doc, &painted);

    doc.clear_active_layer();
    let cleared = document_memory(&doc);
    assert_eq!(
        unique_live_tile_bytes(&doc),
        TILE_BYTES,
        "clearing the paint layer leaves only Paper"
    );
    assert_eq!(cleared.tile_bytes, TILE_BYTES);
    assert!(
        cleared.history_bytes >= TILE_BYTES,
        "history still holds the cleared tiles for undo"
    );

    doc.history = History::new(HISTORY_MEMORY_BUDGET_BYTES);
    let released = document_memory(&doc);
    assert_eq!(released.tile_bytes, TILE_BYTES);
    assert_eq!(
        released.history_bytes, 0,
        "replacing history drops its unique tiles"
    );
    assert_eq!(released.total(), TILE_BYTES);
    assert_eq!(doc.layers[0].tiles().unwrap().get_pixel(0, 0), PAPER_WHITE);
}

#[test]
fn compacting_cold_history_does_not_inflate_the_report() {
    let mut doc = identity_doc();
    for i in 0..20 {
        let x = 40.0 + (i as f32) * 80.0;
        stroke(&mut doc, x, 40.0, x, SIDE as f32 - 40.0);
    }
    let before = document_memory(&doc);
    assert_report_bounds_live_tiles(&doc, &before);
    assert!(doc.history.can_undo());

    let mut freed = 0usize;
    for _ in 0..64 {
        let n = doc.history.compact_cold();
        if n == 0 {
            break;
        }
        freed += n;
    }
    let after = document_memory(&doc);
    assert_report_bounds_live_tiles(&doc, &after);
    assert_eq!(
        after.tile_bytes, before.tile_bytes,
        "compaction is a history-only rewrite"
    );
    assert!(after.history_bytes <= before.history_bytes);
    if freed > 0 {
        assert!(
            after.history_bytes < before.history_bytes
                || doc.history.memory_used() < before.history_bytes + before.tile_bytes,
            "a successful compact must show up in the bytes history still holds"
        );
    }
}
