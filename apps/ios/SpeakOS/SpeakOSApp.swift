import SwiftUI
import UIKit

@main
struct SpeakOSApp: App {
    @State private var model = AppModel()

    init() {
        Fonts.register()
        Appearance.apply()
    }

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(model)
                .font(.uiBody)
                .tint(Theme.primary)
                // The guide designs light first; a reviewed dark pass is later work.
                .preferredColorScheme(.light)
        }
    }
}

enum Appearance {
    /// Ink tab bar with an orange selected item. On iOS 26 the system draws its own floating
    /// bar, which keeps the selected tint but decides the material.
    static func apply() {
        let bar = UITabBarAppearance()
        bar.configureWithOpaqueBackground()
        bar.backgroundColor = UIColor(Theme.ink)
        let normal = UIColor(Color(hex: 0xB4BEE0))
        let selected = UIColor(Theme.orange)
        for item in [bar.stackedLayoutAppearance, bar.inlineLayoutAppearance, bar.compactInlineLayoutAppearance] {
            item.normal.iconColor = normal
            item.normal.titleTextAttributes = [.foregroundColor: normal]
            item.selected.iconColor = selected
            item.selected.titleTextAttributes = [.foregroundColor: selected]
        }
        UITabBar.appearance().standardAppearance = bar
        UITabBar.appearance().scrollEdgeAppearance = bar
    }
}
