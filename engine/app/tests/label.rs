use calumma_app::label_bitmap;

const INK: [u8; 4] = [40, 40, 40, 255];

fn alpha(rgba: &[u8], width: u32, x: u32, y: u32) -> u8 {
    rgba[((y * width + x) * 4 + 3) as usize]
}

#[test]
fn a_turned_label_is_the_flat_one_rotated_a_quarter_turn_counter_clockwise() {
    let Some(flat) = label_bitmap("-1800", 20.0, INK, false) else {
        return;
    };
    let turned = label_bitmap("-1800", 20.0, INK, true).expect("same text, same font");
    assert_eq!((turned.width, turned.height), (flat.height, flat.width));
    for y in 0..flat.height {
        for x in 0..flat.width {
            assert_eq!(
                alpha(&turned.rgba, turned.width, y, flat.width - 1 - x),
                alpha(&flat.rgba, flat.width, x, y),
                "({x}, {y})"
            );
        }
    }
}

/// The line starts at the bottom-left of a turned label, so its ink sits right of and above
/// that point by exactly what the flat label's ink sat below and right of its own start.
#[test]
fn a_turned_labels_ink_offset_follows_the_rotation() {
    let Some(flat) = label_bitmap("400", 20.0, INK, false) else {
        return;
    };
    let turned = label_bitmap("400", 20.0, INK, true).expect("same text, same font");
    assert_eq!(turned.ink_x, flat.ink_y);
    assert_eq!(turned.ink_y, -(flat.ink_x + flat.width as i32));
}

#[test]
fn an_empty_label_has_no_bitmap() {
    assert!(label_bitmap("", 20.0, INK, false).is_none());
}
