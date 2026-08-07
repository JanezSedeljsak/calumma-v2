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
2. **Corner radius.** Controls use `radius.sm` / `radius.md`. **Islands are square** —
   `radius.island` is `0`. Tools, canvas, and layers are full-height bands butted straight
   against each other with no gap and no screen padding; the edge between two surface
   colours is the border. Prefer one radius family; do not mix pill and sharp.
3. **Custom SVG icons only.** Ship icons from `design/icons/`. No icon packs.
   SF Symbols are not the product icon set (system chrome may still use them).
4. **Light and dark.** Every colour has a light and dark value in tokens. The
   shell toggles theme; the engine receives dark-paper via FFI.
5. **Filled controls.** Inputs, buttons, and cards are solid surfaces. Hover and
   active states shift luminance, not outline weight.
6. **Native colour picker.** On macOS use SwiftUI `ColorPicker`, wrapped so its
   chrome matches token radii and surfaces.
7. **Canvas stays Rust.** Anything *drawn on the board* (paper, strokes, shapes, desk grid,
   layer hover outline) is WGSL — the shell never paints board content. Board colours are
   pushed from tokens into the engine, never hardcoded in the shader. Small chrome controls
   may float over the canvas island (zoom pill, bottom-trailing); panels do not.

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
| Desk | `color.desk` | Board background behind the paper (sits *under* the island) |
| Desk grid | `color.deskGrid` | Board grid lines — must stay legible in light mode |
| Paper border | `color.paperBorder` | Ring hugging the paper: dark on light, light on dark |

Desk, desk grid, and paper border are the only tokens the engine consumes. They travel
shell → `calm_engine_set_board_colors` → `PaperUniforms` → `board.wgsl`. Changing the board
look is a `tokens.json` edit, never a shader edit.

Project accent colours are **not** in this table — they are document data owned by
`calumma_core::palette`, served to the shell through `calm_palette_color`.

## Type

- UI sans from the platform (San Francisco on macOS).
- Labels: small, uppercase, tracking from `type.label`.
- Brand wordmark: bold; gradient teal → orange from tokens.

## Landing / New Project

One landscape layout at two sizes — the Landing and the separate New Project window are the
same view, so a project is created the same way whether or not a board is already open.
Form + presets/recents on the left, **Paste Artwork** as a side island whose width tracks
the window between `window.pasteMinWidth` and `window.pasteMaxWidth`. Secondary copy inside
the island drops out when there is no room for it rather than clipping. Unlike the editor
this screen keeps its screen padding — it is a form, not a canvas. No borders between
columns — contrast only. Preset rows and recent rows are filled cards at `radius.md`.

The island is an import target (drop / paste / click), so it also carries a targeted state:
the gradient brightens, no outline.

## Editor

Project tabs sit in a **compact window titlebar** (right of the traffic lights), each with
its project accent dot; clicking the dot opens the rename / recolour card. Top padding is
tight (`space.xs`) so the board starts close under the titlebar.

Tools, canvas, and layers are three **flush full-height bands**, square-cornered, edge to
edge — no gutter, no screen padding, so the canvas gets every pixel that is not a panel.
Shape tools share one button with a mini picker. The **zoom pill** floats bottom-trailing
*inside* the canvas island: `−`, log slider, `+`, percentage, Fit. Layer list rows stay
compact; hovering a row shows a thumbnail popover. Board hover outline remains a dashed
WGSL stroke, not a Swift overlay.

Islands keep their **inner** padding — only the padding *between* them and around the
screen is gone.

## Do not

- Add hairline borders “for clarity”
- Import Lucide / Heroicons / Font Awesome / similar
- Style the canvas with SwiftUI shapes on top of the Metal view
- Duplicate token values in Swift or Rust source
