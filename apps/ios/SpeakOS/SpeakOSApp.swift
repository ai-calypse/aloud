import SwiftUI

@main
struct SpeakOSApp: App {
    @State private var model = AppModel()

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(model)
                .tint(Theme.primary)
                // The guide designs light first; a reviewed dark pass is later work.
                .preferredColorScheme(.light)
        }
    }
}
