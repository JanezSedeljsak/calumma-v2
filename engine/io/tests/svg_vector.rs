use calumma_core::VectorItem;
use calumma_io::decode_svg_vector;

fn path(item: &VectorItem) -> &calumma_core::VectorPath {
    match item {
        VectorItem::Path(p) => p,
        VectorItem::Shape(_) => panic!("an imported SVG is paths only"),
    }
}

#[test]
fn each_svg_path_becomes_one_item_in_page_space() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50">
        <rect x="10" y="10" width="20" height="20" fill="#ff0000"/>
        <circle cx="70" cy="25" r="10" fill="none" stroke="#0000ff" stroke-width="4"/>
    </svg>"##;
    let decoded = decode_svg_vector(svg).expect("a flat-colored SVG converts");
    assert_eq!((decoded.width, decoded.height), (100, 50));
    assert_eq!(decoded.items.len(), 2);
    let rect = path(&decoded.items[0]);
    assert!(rect.fill && rect.closed && !rect.stroke);
    assert_eq!(rect.color, [255, 0, 0, 255]);
    assert_eq!(
        decoded.items[0].geometry_bounds(),
        Some((10.0, 10.0, 30.0, 30.0))
    );
    let circle = path(&decoded.items[1]);
    assert!(!circle.fill && circle.stroke);
    assert_eq!(circle.stroke_width, 4.0);
    assert!(circle.points.len() > 8, "the arc was flattened into chords");
}

#[test]
fn subpaths_stay_one_item_as_rings_with_their_fill_rule() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
        <path fill-rule="evenodd" d="M0 0 H10 V10 H0 Z M3 3 H7 V7 H3 Z" fill="black"/>
    </svg>"##;
    let decoded = decode_svg_vector(svg).unwrap();
    assert_eq!(decoded.items.len(), 1);
    let ring = path(&decoded.items[0]);
    assert_eq!(ring.ring_starts.len(), 1);
    assert!(ring.even_odd);
    assert!(
        decoded.items[0].distance(5.0, 5.0) > 0.0,
        "the hole stays empty"
    );
}

#[test]
fn a_gradient_falls_back_to_pixels() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
        <defs><linearGradient id="g"><stop offset="0" stop-color="red"/><stop offset="1" stop-color="blue"/></linearGradient></defs>
        <rect width="10" height="10" fill="url(#g)"/>
    </svg>"##;
    assert!(decode_svg_vector(svg).is_none());
}

#[test]
fn an_oversized_svg_is_scaled_to_the_import_limit() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="8192" height="4096">
        <rect width="8192" height="4096" fill="black"/>
    </svg>"##;
    let decoded = decode_svg_vector(svg).unwrap();
    assert_eq!((decoded.width, decoded.height), (4096, 2048));
    assert_eq!(
        decoded.items[0].geometry_bounds(),
        Some((0.0, 0.0, 4096.0, 2048.0))
    );
}

#[test]
fn raster_bytes_are_not_an_svg() {
    assert!(decode_svg_vector(b"\x89PNG\r\n\x1a\n").is_none());
}
