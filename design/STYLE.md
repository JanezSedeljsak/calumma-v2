# STYLE.md — Calumma design system

Single visual language for every shell. Tokens live in `design/tokens.json`.
Platforms consume generated theme code — never hardcode hex in UI files.

**Also summarized in `AGENTS.md` (Styling guide + Performance).** Prefer that file for
agent context; keep this file as the expanded design reference.

On macOS, compose `UI/Components.swift` primitives instead of local one-off styling.
UI copy comes from `translations/<lang>.json` (loaded by the platform L10n layer). Dynamic
bits use `{0}`, `{1}`, … filled by `l10n.formatKey(...)`. Visual tokens stay in
`design/tokens.json` → `./manage.py tokens` → `Tokens.*`.

## Rules

1. **No stroke borders.** Separate surfaces with background contrast only.
2. **Minimal corner radius.** Use `radius.sm` / `radius.md` (about 4–6pt) on inputs,
   buttons, islands, and window chrome. Prefer one radius family; do not mix pill and sharp.
3. **Custom SVG icons only.** Ship icons from `design/icons/`. No icon packs.
   SF Symbols are not the product icon set (system chrome may still use them).
4. **Light and dark.** Every colour has a light and dark value in tokens. The
   shell toggles theme; the engine receives dark-paper via FFI.
5. **Filled controls.** Inputs, buttons, and cards are solid surfaces. Hover and
   active states shift luminance, not outline weight.
6. **Native colour picker.** On macOS use SwiftUI `ColorPicker`, wrapped so its
   chrome matches token radii and surfaces.
7. **Canvas stays Rust.** Anything drawn on the board (paper, strokes, shapes,
   layer hover outline) is WGSL. The shell never paints over the Metal layer.

## Hierarchy

| Role | Token | Use |
| --- | --- | --- |
| App background | `color.bg` | Window / landing desk |
| Raised surface | `color.surface` | Cards, inputs, panels |
| Raised hover | `color.surfaceHover` | List row / button hover |
| Text primary | `color.text` | Titles, values |
| Text muted | `color.textMuted` | Labels, paths, timestamps |
| Accent teal | `color.accent.teal` | Create, presets marker, brand start |
| Accent orange | `color.accent.orange` | Recents marker, brand end |
| Danger | `color.danger` | Destructive actions |

## Type

- UI sans from the platform (San Francisco on macOS).
- Labels: small, uppercase, tracking from `type.label`.
- Brand wordmark: bold; gradient teal → orange from tokens.

## Landing

Split layout: form + presets/recents on the left; hero artwork on the right.
No borders between columns — contrast only. Preset rows and recent rows are
filled cards at `radius.md`.

## Editor

Project tabs sit in the **window titlebar** (right of the traffic lights). Tool
rail and layers are floating **islands** over the Metal board (margin inset), not
edge-docked columns. Shape tools share one button with a mini picker. Zoom uses a
slider. Layer list rows stay compact; hovering a row shows a thumbnail popover.
Board hover outline remains a dashed WGSL stroke, not a Swift overlay.

## Do not

- Add hairline borders “for clarity”
- Import Lucide / Heroicons / Font Awesome / similar
- Style the canvas with SwiftUI shapes on top of the Metal view
- Duplicate token values in Swift or Rust source
