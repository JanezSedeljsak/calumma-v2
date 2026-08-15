use calumma_core::filters::*;

#[test]
fn neutral_adjustments_are_a_no_op() {
    let adj = Adjustments::default();
    assert!(adj.is_neutral());
    assert_eq!(apply([12, 200, 88], &adj), [12, 200, 88]);
}

#[test]
fn brightness_lightens_every_channel() {
    let adj = Adjustments {
        brightness: 0.2,
        ..Adjustments::default()
    };
    let out = apply([100, 100, 100], &adj);
    assert!(out[0] > 100 && out[1] > 100 && out[2] > 100);
}

#[test]
fn contrast_pushes_values_away_from_midpoint() {
    let adj = Adjustments {
        contrast: 0.5,
        ..Adjustments::default()
    };
    let bright = apply([200, 200, 200], &adj);
    let dark = apply([50, 50, 50], &adj);
    assert!(bright[0] > 200);
    assert!(dark[0] < 50);
}

#[test]
fn saturation_minus_one_desaturates_to_gray() {
    let adj = Adjustments {
        saturation: -1.0,
        ..Adjustments::default()
    };
    let out = apply([200, 50, 50], &adj);
    assert_eq!(out[0], out[1]);
    assert_eq!(out[1], out[2]);
}

#[test]
fn gamma_below_one_darkens_midtones() {
    let adj = Adjustments {
        levels_gamma: 0.5,
        ..Adjustments::default()
    };
    let out = apply([128, 128, 128], &adj);
    assert!(out[0] < 128);
}

#[test]
fn gamma_above_one_lightens_midtones() {
    let adj = Adjustments {
        levels_gamma: 2.0,
        ..Adjustments::default()
    };
    let out = apply([128, 128, 128], &adj);
    assert!(out[0] > 128);
}

#[test]
fn vibrance_after_full_desaturation_keeps_the_hue() {
    let adj = Adjustments {
        saturation: -1.0,
        vibrance: 0.8,
        ..Adjustments::default()
    };
    // Saturation flattens this blue to gray; vibrance must bring back blue, not red.
    let out = apply([40, 60, 200], &adj);
    assert!(out[2] > out[0], "expected a blue cast, got {out:?}");
}

#[test]
fn lut_matches_the_scalar_filter_exactly() {
    let cases = [
        Adjustments::default(),
        Adjustments {
            brightness: 0.2,
            contrast: -0.4,
            levels_gamma: 1.8,
            ..Adjustments::default()
        },
        Adjustments {
            saturation: 0.7,
            vibrance: -0.3,
            contrast: 0.25,
            ..Adjustments::default()
        },
        Adjustments {
            vibrance: 0.9,
            levels_gamma: 0.4,
            ..Adjustments::default()
        },
    ];
    for adj in cases {
        let lut = adj.lut();
        assert_eq!(lut.is_neutral(), adj.is_neutral());
        for v in (0..256).step_by(7) {
            for w in (0..256).step_by(11) {
                let rgb = [v as u8, w as u8, (255 - v) as u8];
                assert_eq!(lut.apply(rgb), apply(rgb, &adj), "{rgb:?} under {adj:?}");
            }
        }
    }
}

#[test]
fn lut_apply_rgba_leaves_alpha_untouched() {
    let adj = Adjustments {
        brightness: 0.3,
        saturation: 0.5,
        ..Adjustments::default()
    };
    let mut buf = vec![10, 120, 240, 77, 200, 30, 60, 5];
    adj.lut().apply_rgba(&mut buf);
    assert_eq!(buf[3], 77);
    assert_eq!(buf[7], 5);
    assert_eq!([buf[0], buf[1], buf[2]], apply([10, 120, 240], &adj));
    assert_eq!([buf[4], buf[5], buf[6]], apply([200, 30, 60], &adj));
}

#[test]
fn clamped_keeps_values_in_sane_ranges() {
    let adj = Adjustments {
        brightness: 5.0,
        contrast: -9.0,
        levels_gamma: 0.0,
        ..Adjustments::default()
    }
    .clamped();
    assert_eq!(adj.brightness, 1.0);
    assert_eq!(adj.contrast, -1.0);
    assert_eq!(adj.levels_gamma, 0.1);
}

#[test]
fn adjustment_kind_round_trips_through_its_discriminant() {
    for kind in AdjustmentKind::ALL {
        assert_eq!(AdjustmentKind::from_u32(kind.as_u32()), Some(kind));
    }
    assert_eq!(AdjustmentKind::from_u32(5), None);
    assert_eq!(AdjustmentKind::ALL.len(), 5);
}

#[test]
fn nudging_moves_exactly_one_step_per_call() {
    let adj = Adjustments::default();
    let up = adj.nudged(AdjustmentKind::Brightness, 1.0);
    assert!((up.brightness - AdjustmentKind::Brightness.step()).abs() < 1e-6);
    let back = up.nudged(AdjustmentKind::Brightness, -1.0);
    assert!(back.is_neutral());
}

#[test]
fn gamma_nudges_from_one_not_from_zero() {
    let up = Adjustments::default().nudged(AdjustmentKind::LevelsGamma, 1.0);
    assert!((up.levels_gamma - (1.0 + AdjustmentKind::LevelsGamma.step())).abs() < 1e-6);
    assert!(!up.is_neutral());
}

#[test]
fn gamma_uses_a_coarser_step_than_the_other_four() {
    let gamma = AdjustmentKind::LevelsGamma.step();
    for kind in AdjustmentKind::ALL {
        if kind == AdjustmentKind::LevelsGamma {
            continue;
        }
        assert!(kind.step() < gamma, "{kind:?} should step less than gamma");
    }
}

#[test]
fn nudging_touches_only_its_own_channel() {
    let base = Adjustments::default();
    for kind in AdjustmentKind::ALL {
        let next = base.nudged(kind, 1.0);
        for other in AdjustmentKind::ALL {
            if other == kind {
                assert_ne!(next.value(other), base.value(other), "{kind:?}");
            } else {
                assert_eq!(next.value(other), base.value(other), "{kind:?}/{other:?}");
            }
        }
    }
}

#[test]
fn nudging_saturates_at_the_clamp_instead_of_running_away() {
    let mut adj = Adjustments::default();
    for _ in 0..200 {
        adj = adj.nudged(AdjustmentKind::Contrast, 1.0);
    }
    assert_eq!(adj.contrast, 1.0);
    for _ in 0..200 {
        adj = adj.nudged(AdjustmentKind::LevelsGamma, -1.0);
    }
    assert_eq!(adj.levels_gamma, 0.1);
}
