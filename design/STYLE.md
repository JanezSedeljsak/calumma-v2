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

1. **Islands and two kinds of control carry a thin border.** Every `CalmIsland` (tools,
   canvas, layers, Paste Artwork, the zoom pill) is stroked with `color.islandBorder` —
   a subtle, low-alpha tint of the theme's edge colour, not a hard line.

   **Text/number inputs and list rows** also carry a border, at `color.controlBorder`:
   a separate, stronger token, because `islandBorder` is tuned for a large rounded
   island edge and at control scale reads as dirt rather than as an edge. An input
   needs a visible hit target — where do I click to type — and a list row needs an
   edge to make a stack of rows legible as a stack. Contrast alone does neither.
   Applied via `calmSurface(bordered: true)`; a focused input swaps in
   `color.controlFocusBorder` (accent-tinted), because once every input has a resting
   border, a focus ring that is *also* just a border is invisible.

   Everywhere else, surfaces still separate by background contrast only: **no borders
   on buttons, chips, swatches, the tool grid, or sliders.** That is what this rule
   still forbids, and it is why the input/row carve-out is written down rather than
   generalised — "borders on everything" is the outcome this rule exists to prevent.

   Also allowed: a **section separator inside an island** (`CalmDivider`, used by the
   tools panel to split tools / tool options / colour / AI): a 1px `color.islandBorder`
   rule. It separates *stacked sections of one island*, which contrast alone cannot do —
   it is not an outline around a control.
2. **Corner radius.** Controls use `radius.sm` / `radius.md`. Islands use `radius.island`
   (rounded, not square). Tools, canvas, and layers sit apart with a minimal gap
   (`space.sm`) and a minimal margin from the window edge (`space.sm`), tightened to
   `space.xs` along the top edge — the titlebar already separates the islands from the
   chrome above, so a full margin there reads as a dead band. They no longer butt flush
   against each other. Prefer one radius family; do not mix pill and sharp.
3. **Custom SVG icons only.** Ship icons from `design/icons/`. No icon packs.
   SF Symbols are not the product icon set (system chrome may still use them).
4. **Light and dark.** Every colour has a light and dark value in tokens. The
   shell toggles theme; the engine receives dark-paper via FFI.
5. **Filled controls.** Inputs, buttons, and cards are solid surfaces. Hover and
   active states shift luminance, not outline weight.
6. **Inline colour picker.** `QuickColorPicker` is the only colour control: two equal
   quick swatches side by side, a saturation/brightness gradient field, a hue slider, and
   a hex field. Both edit the *active* quick swatch. Hue/saturation/brightness are held as
   model state (`AppModel.hsb`), not re-derived from the RGB colour on every read —
   deriving loses the hue as soon as saturation or brightness hits zero, which makes a
   gradient field jump under the cursor.
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
| Desk | `color.desk` | Board background behind the paper. Light: matches island `surface`. Dark: a step darker than window `bg` so the board field reads recessed against raised islands. |
| Desk grid | `color.deskGrid` | Board grid lines — must stay legible in light mode; stay quiet in dark mode so the desk reads with the chrome |
| Paper border | `color.paperBorder` | Ring hugging the paper: dark on light, light on dark |
| Island border | `color.islandBorder` | `CalmIsland` edge, `CalmDivider` section rules |
| Control border | `color.controlBorder` | Resting edge on text/number inputs and list rows — stronger than `islandBorder`, which is tuned for a large island edge |
| Control focus | `color.controlFocusBorder` | The same edge on a focused input; accent-tinted so focus stays visible against the resting border |

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

Project tabs sit in a **compact window titlebar** (right of the traffic lights) inside one
shared capsule with the `+` / extend controls. Selected tab is a soft highlight clipped to
that capsule — not a second nested pill. Each tab carries its workspace accent dot;
clicking the dot opens the rename / recolour card. Top padding is tight (`space.xs`) so the
board starts close under the titlebar.

Tools, canvas, and layers are three **rounded, bordered islands**, full-height, separated
by a minimal gap (`space.sm`) with a matching margin from the window edge (half that under
the titlebar) — no longer flush or square-cornered. The **zoom pill** floats bottom-trailing *inside* the canvas island: `−`, log
slider, `+`, percentage, a fit-to-view icon (tooltip, no label). Layer list rows stay
compact; hovering a row shows a thumbnail popover. Board hover outline remains a dashed
WGSL stroke, not a Swift overlay.

Islands keep their **inner** padding, plus a minimal `space.sm` gap between them and
margin around the screen.

## Do not

- Add hairline borders “for clarity” to buttons, chips, swatches, the tool grid, or
  sliders — the `controlBorder` carve-out in rule 1 is inputs and list rows only
- Import Lucide / Heroicons / Font Awesome / similar
- Style the canvas with SwiftUI shapes on top of the Metal view
- Duplicate token values in Swift or Rust source
