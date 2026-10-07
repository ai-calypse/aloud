import SwiftUI

/// Palette roles from the UI guide. Sky and deep blue are sampled from the logo;
/// orange and yellow are companion accents. Orange and yellow are fills only,
/// always under a dark label, never text on white.
enum Theme {
    static let canvas = Color(hex: 0xF7F4E8)
    static let surface = Color.white
    static let ink = Color(hex: 0x081751)
    static let primary = Color(hex: 0x0243B2)
    static let sky = Color(hex: 0x0D84FE)
    static let orange = Color(hex: 0xFF981A)
    static let yellow = Color(hex: 0xFFE76A)
    static let muted = Color(hex: 0x616B7A)
    static let border = Color(hex: 0xE5E7EB)
    static let error = Color(hex: 0xB42318)

    static let cardRadius: CGFloat = 20
}

extension Color {
    init(hex: UInt32) {
        self.init(
            .sRGB,
            red: Double((hex >> 16) & 0xFF) / 255,
            green: Double((hex >> 8) & 0xFF) / 255,
            blue: Double(hex & 0xFF) / 255,
            opacity: 1
        )
    }
}

extension Font {
    /// Short brand headings: rounded, tied to Dynamic Type through a text style.
    static let brandTitle = Font.system(.largeTitle, design: .rounded, weight: .heavy)
    static let brandHeading = Font.system(.title3, design: .rounded, weight: .bold)
    /// Small status labels only, never paragraphs or names.
    static let brandMono = Font.system(.footnote, design: .monospaced, weight: .medium)
}

extension View {
    func card(padding: CGFloat = 16) -> some View {
        self
            .padding(padding)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Theme.surface, in: RoundedRectangle(cornerRadius: Theme.cardRadius, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: Theme.cardRadius, style: .continuous).stroke(Theme.border, lineWidth: 1))
    }

    func screenBackground() -> some View {
        background(Theme.canvas.ignoresSafeArea())
    }
}

/// The agreed relative-time wording: "just now" under a minute, then the system's.
func ago(_ date: Date) -> String {
    if Date.now.timeIntervalSince(date) < 60 { return "just now" }
    return RelativeDateTimeFormatter().localizedString(for: date, relativeTo: .now)
}

func clock(_ seconds: Int) -> String {
    String(format: "%d:%02d", seconds / 60, seconds % 60)
}
