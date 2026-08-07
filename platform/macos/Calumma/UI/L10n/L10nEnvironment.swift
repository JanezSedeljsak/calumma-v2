import SwiftUI

private struct L10nKey: EnvironmentKey {
    static let defaultValue = L10nCatalog.load(.en)
}

extension EnvironmentValues {
    var l10n: L10nCatalog {
        get { self[L10nKey.self] }
        set { self[L10nKey.self] = newValue }
    }
}

extension View {
    func l10n(_ catalog: L10nCatalog) -> some View {
        environment(\.l10n, catalog)
    }
}
