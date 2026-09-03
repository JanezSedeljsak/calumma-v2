//! What the machine the app opened on costs the renderer, and how that combines with what the
//! OS is asking for right now.
//!
//! Two axes on the same knobs. [`DeviceTier`] is a **floor**, decided once from the adapter and
//! never moved: a weak GPU wants a smaller tile atlas and a shorter prefetch margin for the
//! whole session, whatever memory is doing. [`MemoryPressureLevel`] is a **ceiling** the OS
//! moves at runtime. Because they want the same two numbers, they must not each set them behind
//! the other's back — [`GpuBudget`] is the one place that answers, and it answers with the
//! stricter of the two.
//!
//! Nothing here knows about wgpu: `core` stays free of platform dependencies (`manage.py
//! purity`), so the render crate maps its adapter onto [`GpuKind`] and asks.

use crate::limits::{
    DOWNLEVEL_TEXTURE_ARRAY_LAYERS, FRAME_HINT_DISPLAY_MAX, FRAME_HINT_LOW_TIER_FPS,
    GPU_TILE_RETENTION_MARGIN_TILES, TILE_ATLAS_MAX_CAPACITY,
};
use crate::memory_pressure::{MemoryPressureLevel, PressureState, PressureTransition};

/// The adapter, reduced to what a tier decision actually turns on. Mirrors `wgpu::DeviceType`
/// minus the distinctions nothing here reads — a virtual GPU is classified by its limits like
/// any other, and `Other` is an adapter that declined to say.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GpuKind {
    Discrete,
    Integrated,
    /// A software rasterizer. Whatever its reported limits, it is not going to keep up.
    Software,
    #[default]
    Other,
}

/// How much GPU the machine has, as far as anything here needs to care. Two levels rather than
/// four: a tier only earns a row when something reads it, and Low/Standard is what the available
/// signal — the adapter's kind and its array-layer limit — can honestly support.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum DeviceTier {
    Low,
    #[default]
    Standard,
}

impl DeviceTier {
    /// A software rasterizer is `Low` outright. An integrated GPU is `Low` only when it *also*
    /// reports a downlevel array-layer limit — Apple Silicon reports `Integrated` with a large
    /// one, and the tier must not punish the machine the app is built on.
    pub fn classify(kind: GpuKind, max_texture_array_layers: u32) -> Self {
        match kind {
            GpuKind::Software => Self::Low,
            GpuKind::Integrated | GpuKind::Other
                if max_texture_array_layers <= DOWNLEVEL_TEXTURE_ARRAY_LAYERS =>
            {
                Self::Low
            }
            _ => Self::Standard,
        }
    }

    /// Tiles kept resident beyond the visible span, so a small pan re-uploads nothing.
    pub fn retention_margin_tiles(self) -> i32 {
        match self {
            Self::Standard => GPU_TILE_RETENTION_MARGIN_TILES,
            Self::Low => 1,
        }
    }

    pub fn atlas_max_capacity(self) -> u32 {
        match self {
            Self::Standard => TILE_ATLAS_MAX_CAPACITY,
            Self::Low => (TILE_ATLAS_MAX_CAPACITY / 4).max(1),
        }
    }

    /// The fastest the board asks to be drawn while something is in flight. A GPU that cannot
    /// hold the panel's rate does not gain anything by being asked to try: the frames arrive
    /// late and unevenly, and the work behind the ones that miss is spent either way. Pacing it
    /// at a rate it can actually hold is what makes a gesture feel even.
    pub fn frame_hint_ceiling(self) -> u32 {
        match self {
            Self::Standard => FRAME_HINT_DISPLAY_MAX,
            Self::Low => FRAME_HINT_LOW_TIER_FPS,
        }
    }

    /// Whether to ask the driver to favour a smaller footprint over throughput when creating the
    /// device (`wgpu::MemoryHints`). The render crate maps this; `core` does not name the type.
    pub fn prefers_small_allocations(self) -> bool {
        self == Self::Low
    }
}

/// The renderer's single source of truth for anything both axes want. Holds the fixed tier and
/// the live [`PressureState`], and answers with the stricter of the two.
#[derive(Clone, Copy, Debug, Default)]
pub struct GpuBudget {
    tier: DeviceTier,
    pressure: PressureState,
}

impl GpuBudget {
    pub fn new(tier: DeviceTier) -> Self {
        Self {
            tier,
            pressure: PressureState::default(),
        }
    }

    pub fn tier(&self) -> DeviceTier {
        self.tier
    }

    pub fn pressure(&self) -> MemoryPressureLevel {
        self.pressure.effective()
    }

    /// Forwards one OS report to the hysteresis machine, returning what changed so the caller
    /// knows which side effects it owes.
    pub fn report_pressure(&mut self, level: MemoryPressureLevel) -> PressureTransition {
        self.pressure.report(level)
    }

    pub fn retention_margin_tiles(&self) -> i32 {
        self.tier
            .retention_margin_tiles()
            .min(self.pressure.effective().retention_margin_tiles())
    }

    pub fn atlas_max_capacity(&self) -> u32 {
        self.tier
            .atlas_max_capacity()
            .min(self.pressure.effective().atlas_max_capacity())
    }

    pub fn frame_hint_ceiling(&self) -> u32 {
        self.tier.frame_hint_ceiling()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Apple Silicon is the case this must not get wrong: it reports an integrated GPU, and a
    /// tier that read `Integrated` alone would put every Mac the app ships on into `Low`.
    #[test]
    fn an_integrated_gpu_is_judged_by_its_limits_and_not_by_being_integrated() {
        assert_eq!(
            DeviceTier::classify(GpuKind::Integrated, 2048),
            DeviceTier::Standard
        );
        assert_eq!(
            DeviceTier::classify(GpuKind::Integrated, DOWNLEVEL_TEXTURE_ARRAY_LAYERS),
            DeviceTier::Low
        );
    }

    #[test]
    fn a_discrete_gpu_is_standard_and_a_software_one_never_is() {
        assert_eq!(
            DeviceTier::classify(GpuKind::Discrete, DOWNLEVEL_TEXTURE_ARRAY_LAYERS),
            DeviceTier::Standard,
            "a discrete part with a modest array limit is still a discrete part"
        );
        assert_eq!(
            DeviceTier::classify(GpuKind::Software, u32::MAX),
            DeviceTier::Low,
            "whatever it claims, it is not going to keep up"
        );
    }

    /// The regression guard for the whole slice: on the machine the app actually runs on, with
    /// no pressure reported, every number has to be exactly what it was before a tier existed.
    #[test]
    fn a_standard_device_under_no_pressure_costs_exactly_what_it_always_did() {
        let budget = GpuBudget::new(DeviceTier::Standard);

        assert_eq!(
            budget.retention_margin_tiles(),
            GPU_TILE_RETENTION_MARGIN_TILES
        );
        assert_eq!(budget.atlas_max_capacity(), TILE_ATLAS_MAX_CAPACITY);
        assert_eq!(budget.frame_hint_ceiling(), FRAME_HINT_DISPLAY_MAX);
    }

    /// Neither axis may set a knob behind the other's back, so the answer is the stricter one
    /// over the whole product of the two enums.
    #[test]
    fn the_budget_is_the_stricter_of_the_tier_and_the_pressure() {
        for tier in [DeviceTier::Low, DeviceTier::Standard] {
            for level in [
                MemoryPressureLevel::Normal,
                MemoryPressureLevel::Warn,
                MemoryPressureLevel::Critical,
            ] {
                let mut budget = GpuBudget::new(tier);
                budget.report_pressure(level);

                assert_eq!(
                    budget.retention_margin_tiles(),
                    tier.retention_margin_tiles()
                        .min(level.retention_margin_tiles()),
                    "{tier:?} / {level:?}"
                );
                assert_eq!(
                    budget.atlas_max_capacity(),
                    tier.atlas_max_capacity().min(level.atlas_max_capacity()),
                    "{tier:?} / {level:?}"
                );
            }
        }
    }

    /// A low tier tightens residency on its own, with the OS reporting nothing at all — that is
    /// the difference between a floor and a ceiling.
    #[test]
    fn a_low_tier_tightens_residency_before_the_os_asks_for_anything() {
        let budget = GpuBudget::new(DeviceTier::Low);

        assert_eq!(budget.pressure(), MemoryPressureLevel::Normal);
        assert!(budget.retention_margin_tiles() < GPU_TILE_RETENTION_MARGIN_TILES);
        assert!(budget.atlas_max_capacity() < TILE_ATLAS_MAX_CAPACITY);
        assert_eq!(budget.frame_hint_ceiling(), FRAME_HINT_LOW_TIER_FPS);
    }

    /// Taking the min must not swallow the hysteresis `PressureState` owns: escalation still
    /// applies on the next report, recovery still needs several.
    #[test]
    fn combining_does_not_defeat_the_pressure_hysteresis() {
        let mut budget = GpuBudget::new(DeviceTier::Standard);

        budget.report_pressure(MemoryPressureLevel::Critical);
        assert_eq!(budget.pressure(), MemoryPressureLevel::Critical);

        budget.report_pressure(MemoryPressureLevel::Normal);
        assert_eq!(
            budget.pressure(),
            MemoryPressureLevel::Critical,
            "one good report is not a recovery"
        );
        budget.report_pressure(MemoryPressureLevel::Normal);
        budget.report_pressure(MemoryPressureLevel::Normal);
        assert_eq!(budget.pressure(), MemoryPressureLevel::Normal);
    }
}
