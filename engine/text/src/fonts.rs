use cosmic_text::fontdb::{Family, Style, Weight};
use cosmic_text::{FontSystem, SwashCache};
use std::sync::{Mutex, MutexGuard, OnceLock};

/// One font database and one glyph cache for the whole process. Scanning the system font
/// directories costs real time on first touch, and every rasterized glyph is worth keeping,
/// so both are built once, lazily, and shared behind a mutex — the same shape the engine
/// already uses for its document `Inner`.
pub struct TextEngine {
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
}

fn engine() -> &'static Mutex<TextEngine> {
    static ENGINE: OnceLock<Mutex<TextEngine>> = OnceLock::new();
    ENGINE.get_or_init(|| {
        Mutex::new(TextEngine {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
        })
    })
}

/// A panic while laying one run out must not take text rendering down for the session, so a
/// poisoned lock is recovered rather than propagated — the font database is immutable after
/// load and the glyph cache is pure cache, so neither can be left half-written.
pub fn with_engine<R>(f: impl FnOnce(&mut TextEngine) -> R) -> R {
    let mut guard: MutexGuard<'_, TextEngine> = engine().lock().unwrap_or_else(|e| e.into_inner());
    f(&mut guard)
}

/// One row of the font picker: a family a person would recognise, plus whether the system
/// actually ships a bold or italic cut of it. Without that last part the shell would have to
/// offer B and I on families that have neither, and cosmic-text would answer with a
/// synthesised face that does not look like the font.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontFamily {
    pub name: String,
    pub bold: bool,
    pub italic: bool,
}

const BOLD_THRESHOLD: u16 = 600;

/// Every installed family, resolved once. Enumerating the database is cheap next to loading
/// it, but the answer cannot change inside a session and both the picker and every
/// `family_exists` check read it, so it is built once and answered by binary search
/// afterwards rather than by walking several thousand faces per question.
fn registry() -> &'static [FontFamily] {
    static REGISTRY: OnceLock<Vec<FontFamily>> = OnceLock::new();
    REGISTRY.get_or_init(build_registry)
}

/// Names beginning with a dot are the OS's own private faces (`.Aqua Kana`, `.SF NS`) —
/// installed and usable, but never something a user picks by name, so they stay out.
/// Families are folded case-insensitively: a face naming itself `ARIAL` and one naming
/// itself `Arial` are one row in the picker, not two.
fn build_registry() -> Vec<FontFamily> {
    let mut rows: Vec<FontFamily> = with_engine(|engine| {
        engine
            .font_system
            .db()
            .faces()
            .filter_map(|face| {
                let name = face.families.first().map(|(name, _)| name.clone())?;
                if name.starts_with('.') || name.trim().is_empty() {
                    return None;
                }
                Some(FontFamily {
                    name,
                    bold: face.weight.0 >= BOLD_THRESHOLD,
                    italic: face.style != Style::Normal,
                })
            })
            .collect()
    });
    rows.sort_by_key(|face| sort_key(&face.name));
    rows.dedup_by(|face, kept| {
        if sort_key(&face.name) != sort_key(&kept.name) {
            return false;
        }
        kept.bold |= face.bold;
        kept.italic |= face.italic;
        true
    });
    rows
}

fn sort_key(name: &str) -> String {
    name.to_lowercase()
}

fn find(name: &str) -> Option<&'static FontFamily> {
    let key = sort_key(name.trim());
    let index = registry()
        .binary_search_by(|row| sort_key(&row.name).cmp(&key))
        .ok()?;
    registry().get(index)
}

/// Families a person would recognise, in the order a picker should show them.
pub fn families() -> Vec<String> {
    registry().iter().map(|row| row.name.clone()).collect()
}

pub fn family_count() -> usize {
    registry().len()
}

pub fn family_at(index: usize) -> Option<&'static FontFamily> {
    registry().get(index)
}

pub fn family_exists(name: &str) -> bool {
    find(name).is_some()
}

/// The bold and italic cuts a family really ships, `(false, false)` for a family that is not
/// installed at all.
pub fn family_styles(name: &str) -> (bool, bool) {
    find(name).map_or((false, false), |row| (row.bold, row.italic))
}

/// The name as the database spells it, so a family set from a project file or a script lands
/// in the picker's own casing instead of shadowing the row it means.
pub fn canonical_family(name: &str) -> Option<&'static str> {
    find(name).map(|row| row.name.as_str())
}

/// Fallbacks tried in order when the font database's own generic mapping names a family
/// that is not actually installed — which is the normal case outside fontconfig systems.
const DEFAULT_FAMILY_PREFERENCE: [&str; 6] = [
    "Helvetica Neue",
    "Helvetica",
    "Arial",
    "Segoe UI",
    "DejaVu Sans",
    "Liberation Sans",
];

/// The family a new text layer starts in: whatever the database calls sans-serif if that is
/// really installed, else the first recognisable fallback, else anything at all.
pub fn default_family() -> String {
    let generic = with_engine(|engine| {
        engine
            .font_system
            .db()
            .family_name(&Family::SansSerif)
            .to_string()
    });
    if family_exists(&generic) {
        return generic;
    }
    for candidate in DEFAULT_FAMILY_PREFERENCE {
        if family_exists(candidate) {
            return candidate.to_string();
        }
    }
    registry()
        .first()
        .map(|row| row.name.clone())
        .unwrap_or(generic)
}

pub(crate) fn weight_of(bold: bool) -> Weight {
    if bold {
        Weight::BOLD
    } else {
        Weight::NORMAL
    }
}

pub(crate) fn style_of(italic: bool) -> Style {
    if italic {
        Style::Italic
    } else {
        Style::Normal
    }
}
