//! What an OS memory-pressure signal means for GPU tile residency — the product table shipped
//! as plan 22, plus the hysteresis around it. Only the
//! shell can receive the underlying OS signal (a dispatch memory-pressure source on macOS), so
//! this is the inbound side: the shell forwards a level, never a tile count or byte budget, and
//! core owns what each level costs the atlas.

use crate::limits::{GPU_TILE_RETENTION_MARGIN_TILES, TILE_ATLAS_MAX_CAPACITY};
use num_enum::{IntoPrimitive, TryFromPrimitive};

/// Mirrors the three levels the OS itself reports (`DISPATCH_SOURCE_TYPE_MEMORYPRESSURE` on
/// macOS: `.normal` / `.warn` / `.critical`). Ordered by severity so `Ord` gives "worse than" /
/// "better than" for free — [`PressureState`] leans on that to decide whether a report is an
/// escalation or a recovery. `IntoPrimitive`/`TryFromPrimitive` mirror how `Tool` crosses the
/// FFI boundary — a plain `u32`, not a declared C enum type.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, IntoPrimitive, TryFromPrimitive,
)]
#[repr(u32)]
pub enum MemoryPressureLevel {
    #[default]
    Normal,
    Warn,
    Critical,
}

impl MemoryPressureLevel {
    pub fn from_u32(v: u32) -> Option<Self> {
        Self::try_from(v).ok()
    }

    /// Tiles retained beyond the visible span. `Normal` keeps today's
    /// [`GPU_TILE_RETENTION_MARGIN_TILES`]; `Critical` drops to the visible set only.
    pub fn retention_margin_tiles(self) -> i32 {
        match self {
            Self::Normal => GPU_TILE_RETENTION_MARGIN_TILES,
            Self::Warn => 1,
            Self::Critical => 0,
        }
    }

    /// Growth ceiling for the tile atlas, as a fraction of [`TILE_ATLAS_MAX_CAPACITY`]. Lowering
    /// this only stops *future* growth — an atlas already bigger than the new ceiling is not
    /// forcibly shrunk by this alone. See [`PressureState::should_shrink_atlas`] for the one
    /// level that does shrink it.
    pub fn atlas_max_capacity(self) -> u32 {
        match self {
            Self::Normal => TILE_ATLAS_MAX_CAPACITY,
            Self::Warn => (TILE_ATLAS_MAX_CAPACITY / 2).max(1),
            Self::Critical => (TILE_ATLAS_MAX_CAPACITY / 4).max(1),
        }
    }
}

/// Consecutive *improving* reports required before a raised level is actually relaxed. Pressure
/// signals oscillate, and flipping the retention margin every time the OS blips back to normal
/// would thrash re-uploads worse than just sitting at the tighter margin — the same
/// enter/exit-threshold asymmetry `OVERVIEW_ENTER_TILE_THRESHOLD` /
/// `OVERVIEW_EXIT_TILE_THRESHOLD` already use for the overview path. Escalation, by contrast,
/// always applies on the very next report — there is no case where reacting slower to *more*
/// pressure is the safe default.
const MEMORY_PRESSURE_RECOVERY_SIGNALS: u32 = 3;

/// Consecutive `Critical` reports required before the atlas texture is actually recreated
/// smaller, on top of the margin/eviction response that applies immediately. Shrinking is a
/// full recreate-and-re-upload — the same cost as growth — so a single transient spike should
/// tighten residency without paying for it; only sustained pressure earns the expensive step.
const MEMORY_PRESSURE_SHRINK_SIGNALS: u32 = 3;

/// The hysteresis machine behind the table in docs/plans/22: tracks what level is actually in
/// effect (as opposed to what was last reported) and how long `Critical` has persisted.
#[derive(Clone, Copy, Debug, Default)]
pub struct PressureState {
    effective: MemoryPressureLevel,
    /// Level a recovery is in progress towards, with how many consecutive reports have asked
    /// for it. Reset the moment a report doesn't match, or effective catches up to it.
    recovering_to: Option<(MemoryPressureLevel, u32)>,
    critical_streak: u32,
}

/// What a [`PressureState::report`] call changed, so the renderer knows which side effects it
/// owes: `clear_layer_cache()` on any effective-level change (the retention margin moved, so
/// `cached_retained_span` would go stale), and an atlas shrink only when `shrink` is true.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PressureTransition {
    pub effective_changed: bool,
    pub shrink: bool,
}

impl PressureState {
    pub fn effective(&self) -> MemoryPressureLevel {
        self.effective
    }

    /// Records one report from the shell, returning what changed. Worsening always takes effect
    /// immediately; relaxing needs [`MEMORY_PRESSURE_RECOVERY_SIGNALS`] consecutive reports at
    /// (or below) the same level first.
    pub fn report(&mut self, level: MemoryPressureLevel) -> PressureTransition {
        if level >= self.effective {
            self.recovering_to = None;
            let effective_changed = level != self.effective;
            self.effective = level;
            let shrink = self.note_critical_streak();
            return PressureTransition {
                effective_changed,
                shrink,
            };
        }

        // `level < self.effective`: an improvement. Count consecutive asks for it before
        // actually relaxing, and reset the streak if the ask changes mid-recovery.
        let streak = match self.recovering_to {
            Some((pending, count)) if pending == level => count + 1,
            _ => 1,
        };
        if streak >= MEMORY_PRESSURE_RECOVERY_SIGNALS {
            self.recovering_to = None;
            self.effective = level;
            self.critical_streak = 0;
            PressureTransition {
                effective_changed: true,
                shrink: false,
            }
        } else {
            self.recovering_to = Some((level, streak));
            PressureTransition::default()
        }
    }

    fn note_critical_streak(&mut self) -> bool {
        if self.effective == MemoryPressureLevel::Critical {
            self.critical_streak += 1;
        } else {
            self.critical_streak = 0;
        }
        self.critical_streak >= MEMORY_PRESSURE_SHRINK_SIGNALS
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use MemoryPressureLevel::*;

    #[test]
    fn starts_normal() {
        assert_eq!(PressureState::default().effective(), Normal);
    }

    #[test]
    fn escalation_is_immediate() {
        let mut state = PressureState::default();
        let t = state.report(Warn);
        assert_eq!(state.effective(), Warn);
        assert!(t.effective_changed);
        assert!(!t.shrink);

        let t = state.report(Critical);
        assert_eq!(state.effective(), Critical);
        assert!(t.effective_changed);
    }

    #[test]
    fn recovery_needs_several_consecutive_reports() {
        let mut state = PressureState::default();
        state.report(Critical);

        for _ in 0..MEMORY_PRESSURE_RECOVERY_SIGNALS - 1 {
            let t = state.report(Normal);
            assert_eq!(
                state.effective(),
                Critical,
                "not enough consecutive asks yet"
            );
            assert!(!t.effective_changed);
        }
        let t = state.report(Normal);
        assert_eq!(state.effective(), Normal);
        assert!(t.effective_changed);
    }

    #[test]
    fn recovering_to_a_different_target_restarts_the_streak() {
        let mut state = PressureState::default();
        state.report(Critical);
        state.report(Warn); // streak=1 towards Warn
        let t = state.report(Normal); // different target: streak resets to 1 towards Normal
        assert!(!t.effective_changed);
        assert_eq!(state.effective(), Critical);
    }

    #[test]
    fn shrink_only_after_sustained_critical() {
        let mut state = PressureState::default();
        for i in 1..MEMORY_PRESSURE_SHRINK_SIGNALS {
            let t = state.report(Critical);
            assert!(!t.shrink, "report {i} should not shrink yet");
        }
        let t = state.report(Critical);
        assert!(t.shrink);
    }

    #[test]
    fn dropping_out_of_critical_resets_the_shrink_streak() {
        let mut state = PressureState::default();
        state.report(Critical);
        state.report(Critical);
        for _ in 0..MEMORY_PRESSURE_RECOVERY_SIGNALS {
            state.report(Warn);
        }
        assert_eq!(state.effective(), Warn);

        state.report(Critical);
        let t = state.report(Critical);
        assert!(
            !t.shrink,
            "streak restarted after leaving Critical, so two reports is not enough again"
        );
    }
}
