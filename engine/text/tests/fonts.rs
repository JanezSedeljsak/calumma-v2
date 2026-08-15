use calumma_text::{
    canonical_family, default_family, families, family_at, family_count, family_exists,
    family_styles, measure, rasterize, TextRun,
};

fn styled(text: &str, bold: bool, italic: bool) -> TextRun {
    TextRun {
        text: text.to_string(),
        bold,
        italic,
        size: 32.0,
        ..TextRun::default()
    }
    .clamped()
}

#[test]
fn the_system_font_list_is_read_from_the_font_database() {
    let list = families();
    assert!(list.len() > 5, "expected system fonts, got {}", list.len());
    assert_eq!(list.len(), family_count());
    assert!(!list.iter().any(|name| name.starts_with('.')));
    assert!(!list.iter().any(|name| name.trim().is_empty()));
}

#[test]
fn the_list_is_sorted_and_carries_each_family_once() {
    let list = families();
    assert!(list
        .windows(2)
        .all(|w| w[0].to_lowercase() < w[1].to_lowercase()));
}

#[test]
fn every_row_reports_its_own_styles() {
    for index in 0..family_count() {
        let family = family_at(index).expect("row in range");
        assert_eq!(family_styles(&family.name), (family.bold, family.italic));
        assert!(family_exists(&family.name));
    }
    assert!(family_at(family_count()).is_none(), "out of range is none");
}

#[test]
fn a_family_is_found_however_it_is_spelled() {
    let name = default_family();
    assert_eq!(canonical_family(&name.to_uppercase()), Some(name.as_str()));
    assert_eq!(
        canonical_family(&format!("  {name}  ")),
        Some(name.as_str())
    );
    assert!(canonical_family("No Such Family At All").is_none());
    assert!(!family_exists("No Such Family At All"));
    assert!(!family_exists(""));
}

#[test]
fn the_default_family_is_one_the_engine_can_draw() {
    let family = default_family();
    assert!(family_exists(&family));
    assert!(rasterize(
        &TextRun {
            text: "default".into(),
            family,
            ..TextRun::default()
        }
        .clamped()
    )
    .is_some());
}

#[test]
fn at_least_one_family_ships_both_a_bold_and_an_italic_cut() {
    let styled = (0..family_count())
        .filter_map(family_at)
        .filter(|family| family.bold && family.italic)
        .count();
    assert!(styled > 0, "no family reported bold and italic cuts");
}

#[test]
fn bold_and_italic_change_the_shaped_result() {
    let family = default_family();
    if family_styles(&family) != (true, true) {
        return;
    }
    let plain = TextRun {
        family: family.clone(),
        ..styled("Handgloves", false, false)
    };
    let bold = TextRun {
        bold: true,
        ..plain.clone()
    };
    let italic = TextRun {
        italic: true,
        ..plain.clone()
    };
    assert!(measure(&bold).0 > measure(&plain).0, "bold sets wider");
    assert_ne!(
        rasterize(&italic).map(|r| r.rgba),
        rasterize(&plain).map(|r| r.rgba),
        "italic draws different ink"
    );
}

#[test]
fn an_uninstalled_family_still_rasterizes_through_a_fallback() {
    let run = TextRun {
        family: "Definitely Not Installed".into(),
        ..styled("fallback", false, false)
    };
    assert!(
        rasterize(&run).is_some(),
        "a missing family must not blank the layer"
    );
}
