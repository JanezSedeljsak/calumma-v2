import SwiftUI

/// The guides on this board as a list you can type into, opened from the ruler corner. Dragging
/// off a ruler is still the quick way to place one; this is the way to put one at exactly 240,
/// or to find the one you dropped somewhere off screen.
///
/// The engine owns every guide and every rule about them — a typed position is clamped onto the
/// paper by `Document::set_guide_position`, a duplicate is refused by `add_guide`. This reads
/// the list back after each edit rather than keeping its own copy, because an edit can change
/// what the indices mean.
struct GuidesCard: View {
    @EnvironmentObject private var app: AppModel
    @Environment(\.themeColors) private var colors
    @Environment(\.l10n) private var l10n

    @State private var entries: [GuideEntry] = []
    @State private var draftAxis: CalmGuideAxis = .horizontal
    @State private var draftOffset = 0
    /// Which row's swatch has its palette open, by guide index. One at a time, and by index
    /// rather than a flag per row, because the rows are rebuilt from the engine after every edit.
    @State private var colorPickerRow: Int?

    /// How large the guides modal is presented at, given the window it opens over. Wants to be
    /// tall — the list is the point of the panel — and stops short of the window's own edges.
    /// Sized to hold the whole list rather than to fill the window: a document tops out at
    /// `Engine.guidesCap` guides, so past that height the card is empty space.
    static func size(in window: CGFloat) -> CGSize {
        CGSize(width: 380, height: min(500, max(360, window - 260)))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.md) {
            header
            CalmDivider()
            list
            CalmDivider()
            addRow
        }
        .padding(Tokens.Space.lg)
        // No size of its own: it fills the modal it is presented in, which is what lets the list
        // take every point left over. As a popover it could not — `ScrollView` has no height to
        // offer, so the popover sized it to nearly nothing and no ceiling could raise it.
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .calmPanel()
        .onAppear(perform: reload)
        // Every edit here goes through the engine and comes back through this — including the
        // ones that leave the list the same length, which is why it watches the revision rather
        // than the count.
        .onChange(of: app.engine.guidesRevision) { _, _ in reload() }
    }

    private var header: some View {
        HStack {
            CalmText.label(l10n.guides)
            Spacer()
            if !entries.isEmpty {
                Button(l10n.clearGuides) { app.engine.clearGuides() }
                    .buttonStyle(.plain)
                    .font(.system(size: Tokens.TypeSize.label))
                    .foregroundStyle(colors.danger)
                    .calmPointer()
            }
        }
    }

    @ViewBuilder
    private var list: some View {
        if entries.isEmpty {
            VStack(alignment: .leading, spacing: Tokens.Space.xs) {
                CalmText.muted(l10n.noGuides)
                CalmText.muted(l10n.guidesHint)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        } else {
            ScrollView(.vertical) {
                VStack(spacing: Tokens.Space.xs) {
                    ForEach(entries) { entry in
                        row(entry)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .topLeading)
                .calmScrollBars()
            }
            .frame(maxHeight: .infinity)
        }
    }

    private func row(_ entry: GuideEntry) -> some View {
        HStack(spacing: Tokens.Space.sm) {
            // The same toggle the Add row carries, so an existing guide can be moved to the
            // other edge without deleting it and typing it back in. Refused rather than
            // duplicated when the other edge already has a guide there
            // (`Document::set_guide_axis`), which shows as the toggle simply not moving.
            axisToggle(selected: entry.axis) { axis in
                app.engine.setGuideAxis(index: entry.index, axis: axis)
            }
            CalmNumberField(
                value: Binding(
                    get: { Int(entry.position.rounded()) },
                    set: { app.engine.setGuidePosition(index: entry.index, position: Float($0)) }
                ),
                width: 64
            )
            colorWell(entry)
            Spacer()
            Button {
                app.engine.removeGuide(index: entry.index)
            } label: {
                AppIcon.trash(color: colors.danger)
            }
            .buttonStyle(.plain)
            .help(l10n.deleteGuide)
            .calmPointer()
        }
    }

    /// The rule's own color, beside the number that places it. The board palette rather than a
    /// free picker: ten colors already chosen to read against both the desk and white paper is
    /// the same problem a guide has, and the first of them is the color guides start in — so
    /// picking that one is how you put a rule back to the default.
    ///
    /// A popover rather than the palette inline, because a row of ten dots is most of the card's
    /// width and this row already carries three controls.
    private func colorWell(_ entry: GuideEntry) -> some View {
        Button {
            colorPickerRow = entry.index
        } label: {
            CalmDot(color: entry.color, size: 14)
        }
        .buttonStyle(.plain)
        .help(l10n.guideColor)
        .calmPointer()
        .popover(
            isPresented: Binding(
                get: { colorPickerRow == entry.index },
                set: { if !$0 { colorPickerRow = nil } }
            ),
            arrowEdge: .bottom
        ) {
            CalmPaletteRow(colors: Engine.palette, selected: entry.color) { color in
                app.engine.setGuideColor(index: entry.index, color: color)
                colorPickerRow = nil
            }
            .padding(Tokens.Space.sm)
        }
    }

    /// A horizontal rule is placed by how far down it sits, a vertical one by how far across —
    /// so the card names the edge each is measured from rather than the axis it runs along,
    /// which is the thing nobody can keep straight.
    private func label(_ axis: CalmGuideAxis) -> String {
        axis == .horizontal ? l10n.guideTop : l10n.guideLeft
    }

    private var addRow: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.xs) {
            CalmText.label(l10n.guideOffset)
            HStack(spacing: Tokens.Space.sm) {
                axisToggle(selected: draftAxis) { draftAxis = $0 }
                CalmNumberField(value: $draftOffset, width: 64)
                    .onSubmit(add)
                // Not a picker: a new guide always starts in the default color, and this is the
                // swatch that says which one that is. Recoloring is a thing you do to a rule you
                // can already see, on its own row.
                CalmDot(color: Engine.defaultGuideColor, size: 14)
                    .help(l10n.guideColor)
                Spacer()
                // Greyed at the ceiling rather than left to do nothing: `add_guide` refuses a
                // full list silently, and a button that answers a click with nothing is worse
                // than one that says it cannot.
                let full = entries.count >= Engine.guidesCap
                Button(l10n.addGuide, action: add)
                    .buttonStyle(.plain)
                    .font(.system(size: Tokens.TypeSize.label, weight: .semibold))
                    .foregroundStyle(full ? colors.textMuted : colors.accentTeal)
                    .disabled(full)
                    .calmPointer(!full)
                    .help(full ? l10n.guidesFull : l10n.addGuide)
            }
        }
    }

    /// Two chips rather than a picker: there are exactly two axes and both fit, so the choice is
    /// visible instead of hidden behind a menu. Shared by the Add row and every existing row —
    /// setting an axis and changing one are the same control.
    private func axisToggle(
        selected: CalmGuideAxis,
        onPick: @escaping (CalmGuideAxis) -> Void
    ) -> some View {
        HStack(spacing: 0) {
            axisChip(.horizontal, selected: selected, onPick: onPick)
            axisChip(.vertical, selected: selected, onPick: onPick)
        }
        .background(colors.surfaceHover, in: Capsule(style: .continuous))
        .clipShape(Capsule(style: .continuous))
    }

    private func axisChip(
        _ axis: CalmGuideAxis,
        selected current: CalmGuideAxis,
        onPick: @escaping (CalmGuideAxis) -> Void
    ) -> some View {
        let selected = current == axis
        return Button {
            onPick(axis)
        } label: {
            Text(label(axis))
                .font(.system(size: Tokens.TypeSize.label, weight: selected ? .semibold : .medium))
                .foregroundStyle(selected ? colors.text : colors.textMuted)
                .padding(.horizontal, Tokens.Space.sm)
                .frame(height: 22)
                .background(selected ? colors.surface : Color.clear)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .calmPointer()
    }

    private func add() {
        app.engine.addGuide(axis: draftAxis, position: Float(draftOffset))
    }

    private func reload() {
        entries = app.engine.guideList()
    }
}
